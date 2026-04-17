use crate::decoder::{
    compile_event_decode_plan, EventPlanKind, EventTypeInfo, ProtocolDefinition, TypeInfo,
};
use crate::error::DecodeError;
use crate::events::{GameEventField, MessageEventField, TrackerEventField};
use serde_json::Value as JsonValue;
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Clone)]
pub struct ProtocolStore {
    protocols: HashMap<u32, ProtocolDefinition>,
    latest: u32,
}

impl ProtocolStore {
    pub fn load() -> Result<Self, DecodeError> {
        let data = include_str!("../protocols/protocols.json");
        let json: JsonValue = serde_json::from_str(data)
            .map_err(|e| DecodeError::Json(format!("protocol metadata parse failed: {e}")))?;
        let protocols_arr = json
            .get("protocols")
            .and_then(|v| v.as_array())
            .ok_or_else(|| DecodeError::Json("missing protocols array".into()))?;

        let mut map = HashMap::new();
        let mut latest = 0u32;

        for proto in protocols_arr {
            let build = proto
                .get("build")
                .and_then(|v| v.as_u64())
                .ok_or_else(|| DecodeError::Json("missing build".into()))?
                as u32;

            let typeinfos_raw = proto
                .get("typeinfos")
                .and_then(|v| v.as_array())
                .ok_or_else(|| DecodeError::Json("missing typeinfos".into()))?;

            let mut typeinfos = Vec::with_capacity(typeinfos_raw.len());
            for (typeid, item) in typeinfos_raw.iter().enumerate() {
                let entry = item
                    .as_array()
                    .ok_or_else(|| DecodeError::Json("typeinfo entry is not array".into()))?;
                if entry.len() != 2 {
                    return Err(DecodeError::Json("typeinfo entry not length 2".into()));
                }

                let op_name = entry
                    .first()
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| DecodeError::Json("typeinfo name missing".into()))?;

                let args = match &entry[1] {
                    JsonValue::Array(v) => v.clone(),
                    JsonValue::Object(_) => vec![entry[1].clone()],
                    JsonValue::Null => Vec::new(),
                    _ => vec![entry[1].clone()],
                };

                typeinfos.push(TypeInfo::new(typeid, op_name, args)?);
            }

            let game_event_types = parse_event_map(proto.get("game_event_types"))?;
            let message_event_types = parse_event_map(proto.get("message_event_types"))?;
            let tracker_event_types = parse_event_map(proto.get("tracker_event_types"))?;
            let typeinfos: Arc<[TypeInfo]> = typeinfos.into();
            let game_event_typeinfos = build_event_typeinfos(
                &game_event_types,
                typeinfos.as_ref(),
                EventPlanKind::Ordered,
                &mut GameEventField::from_key,
            )?;
            let message_event_typeinfos = build_event_typeinfos(
                &message_event_types,
                typeinfos.as_ref(),
                EventPlanKind::Ordered,
                &mut MessageEventField::from_key,
            )?;
            let tracker_event_typeinfos = build_event_typeinfos(
                &tracker_event_types,
                typeinfos.as_ref(),
                EventPlanKind::Tagged,
                &mut TrackerEventField::from_key,
            )?;

            let build_def = ProtocolDefinition {
                build,
                typeinfos,
                game_event_types,
                message_event_types,
                tracker_event_types,
                game_event_typeinfos,
                message_event_typeinfos,
                tracker_event_typeinfos,
                game_eventid_typeid: to_usize(proto, "game_eventid_typeid")?,
                message_eventid_typeid: to_usize(proto, "message_eventid_typeid")?,
                tracker_eventid_typeid: to_usize_opt(proto, "tracker_eventid_typeid"),
                svaruint32_typeid: to_usize(proto, "svaruint32_typeid")?,
                replay_userid_typeid: to_usize_opt(proto, "replay_userid_typeid"),
                replay_header_typeid: to_usize(proto, "replay_header_typeid")?,
                game_details_typeid: to_usize(proto, "game_details_typeid")?,
                replay_initdata_typeid: to_usize(proto, "replay_initdata_typeid")?,
            };

            latest = latest.max(build);
            map.insert(build, build_def);
        }

        Ok(ProtocolStore {
            protocols: map,
            latest,
        })
    }

    pub fn latest(&self) -> Result<&ProtocolDefinition, DecodeError> {
        self.protocols
            .get(&self.latest)
            .ok_or(DecodeError::ProtocolMissing(self.latest))
    }

    pub fn build(&self, build: u32) -> Result<&ProtocolDefinition, DecodeError> {
        self.protocols
            .get(&build)
            .ok_or(DecodeError::ProtocolMissing(build))
    }

    pub fn closest_build(&self, target: u32) -> Option<u32> {
        if self.protocols.is_empty() {
            return None;
        }

        let mut closest = None;
        let mut best_distance = u32::MAX;

        for build in self.protocols.keys() {
            let distance = build.abs_diff(target);
            if distance < best_distance {
                best_distance = distance;
                closest = Some(*build);
            }
        }

        closest
    }

    pub fn build_or_closest(&self, target: u32) -> Option<&ProtocolDefinition> {
        let exact = self.protocols.get(&target);
        if exact.is_some() {
            return exact;
        }
        self.closest_build(target)
            .and_then(|build| self.protocols.get(&build))
    }

    pub fn known_builds(&self) -> Vec<u32> {
        let mut builds: Vec<u32> = self.protocols.keys().copied().collect();
        builds.sort_unstable();
        builds
    }

    pub fn known_builds_range(&self, min: u32, max: u32) -> Vec<u32> {
        let mut builds = self.known_builds();
        builds.retain(|build| *build >= min && *build <= max);
        builds
    }
}

fn to_usize(proto: &JsonValue, field: &str) -> Result<usize, DecodeError> {
    proto
        .get(field)
        .and_then(|v| v.as_u64())
        .map(|v| v as usize)
        .ok_or_else(|| DecodeError::Json(format!("missing {field}")))
}

fn to_usize_opt(proto: &JsonValue, field: &str) -> Option<usize> {
    proto
        .get(field)
        .and_then(|v| v.as_u64())
        .map(|v| v as usize)
}

fn parse_event_map(value: Option<&JsonValue>) -> Result<Vec<(u32, u32, String)>, DecodeError> {
    let map_obj = value
        .and_then(|v| v.as_object())
        .ok_or_else(|| DecodeError::Json("event map missing".into()))?;

    let mut entries = Vec::with_capacity(map_obj.len());
    for (key, val) in map_obj {
        let event_id = key
            .parse::<u32>()
            .map_err(|_| DecodeError::Json("invalid event map key".into()))?;

        let tuple = val
            .as_array()
            .ok_or_else(|| DecodeError::Json("event map value should be list".into()))?;
        if tuple.len() < 2 {
            return Err(DecodeError::Json("event map tuple short".into()));
        }
        let typeid = tuple[0]
            .as_u64()
            .ok_or_else(|| DecodeError::Json("event map typeid missing".into()))?
            as u32;
        let name = tuple[1].as_str().unwrap_or("<unknown>").to_string();

        entries.push((event_id, typeid, name));
    }

    Ok(entries)
}

fn build_event_typeinfos<F, S>(
    event_types: &[(u32, u32, String)],
    typeinfos: &[TypeInfo],
    plan_kind: EventPlanKind,
    select_field: &mut S,
) -> Result<Arc<[Option<EventTypeInfo<F>>]>, DecodeError>
where
    F: Copy,
    S: FnMut(&str) -> Option<F>,
{
    let max_event_id = event_types
        .iter()
        .map(|(event_id, _, _)| usize::try_from(*event_id))
        .collect::<Result<Vec<usize>, _>>()
        .map_err(|_| DecodeError::Json("event id does not fit in usize".into()))?
        .into_iter()
        .max();

    let Some(max_event_id) = max_event_id else {
        return Ok(Arc::from(Vec::<Option<EventTypeInfo<F>>>::new()));
    };

    let mut lookup = vec![None; max_event_id + 1];
    for (event_id, typeid, name) in event_types {
        let event_index = usize::try_from(*event_id)
            .map_err(|_| DecodeError::Json("event id does not fit in usize".into()))?;
        let type_index = usize::try_from(*typeid)
            .map_err(|_| DecodeError::Json("event typeid does not fit in usize".into()))?;
        let typeinfo = typeinfos
            .get(type_index)
            .cloned()
            .ok_or_else(|| DecodeError::Json(format!("event typeid {type_index} out of range")))?;
        let decode_plan = compile_event_decode_plan(&typeinfo, typeinfos, plan_kind, select_field)?;

        if lookup[event_index].is_some() {
            return Err(DecodeError::Json(format!(
                "duplicate event id {event_index} in protocol map"
            )));
        }

        lookup[event_index] = Some(EventTypeInfo::new(name.clone(), decode_plan));
    }

    Ok(Arc::from(lookup))
}

pub fn build_protocol_store() -> Result<ProtocolStore, DecodeError> {
    ProtocolStore::load()
}
