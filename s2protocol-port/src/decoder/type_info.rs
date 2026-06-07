use crate::error::DecodeError;
use serde_json::Value as JsonValue;
use std::collections::BTreeMap;
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum TypeOp {
    Array,
    BitArray,
    Blob,
    Bool,
    Choice,
    Fourcc,
    Int,
    Null,
    Optional,
    Real32,
    Real64,
    Struct,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct IntBounds {
    min: i64,
    bits: usize,
}

impl IntBounds {
    pub(super) fn min(&self) -> i64 {
        self.min
    }

    pub(super) fn bits(&self) -> usize {
        self.bits
    }
}

#[derive(Debug, Clone)]
pub(super) enum TagLookup<T> {
    Dense {
        min_tag: i128,
        entries: Arc<[Option<T>]>,
    },
    Sparse(Arc<BTreeMap<i128, T>>),
}

impl<T> TagLookup<T> {
    pub(super) fn get(&self, tag: &i128) -> Option<&T> {
        match self {
            Self::Dense { min_tag, entries } => {
                let offset = tag.checked_sub(*min_tag)?;
                let index = usize::try_from(offset).ok()?;
                entries.get(index).and_then(Option::as_ref)
            }
            Self::Sparse(map) => map.get(tag),
        }
    }

    pub(super) fn visit<F>(&self, mut visitor: F) -> Result<(), DecodeError>
    where
        F: FnMut(i128, &T) -> Result<(), DecodeError>,
    {
        match self {
            Self::Dense { min_tag, entries } => {
                for (index, entry) in entries.iter().enumerate() {
                    if let Some(value) = entry {
                        let tag = min_tag
                            .checked_add(index as i128)
                            .ok_or_else(|| DecodeError::Corrupted("tag out of range".into()))?;
                        visitor(tag, value)?;
                    }
                }
            }
            Self::Sparse(map) => {
                for (tag, value) in map.iter() {
                    visitor(*tag, value)?;
                }
            }
        }

        Ok(())
    }
}

impl<T: Clone> TagLookup<T> {
    pub(super) fn new(
        entries: Vec<(i128, T)>,
        duplicate_context: &str,
    ) -> Result<Option<Self>, DecodeError> {
        if entries.is_empty() {
            return Ok(None);
        }

        let mut min_tag = i128::MAX;
        let mut max_tag = i128::MIN;
        let mut sparse = BTreeMap::new();
        for (tag, value) in &entries {
            if sparse.insert(*tag, value.clone()).is_some() {
                return Err(DecodeError::Corrupted(format!("{duplicate_context} {tag}")));
            }
            min_tag = min_tag.min(*tag);
            max_tag = max_tag.max(*tag);
        }

        let dense_len = max_tag
            .checked_sub(min_tag)
            .and_then(|width| width.checked_add(1))
            .and_then(|width| usize::try_from(width).ok());
        let dense_threshold = entries.len().saturating_mul(4).max(16);
        if min_tag >= 0
            && let Some(dense_len) = dense_len
            && dense_len <= dense_threshold
        {
            let mut dense_entries = vec![None; dense_len];
            for (tag, value) in entries {
                let index = usize::try_from(tag - min_tag)
                    .map_err(|_| DecodeError::Corrupted("tag index out of range".into()))?;
                dense_entries[index] = Some(value);
            }
            return Ok(Some(Self::Dense {
                min_tag,
                entries: Arc::from(dense_entries),
            }));
        }

        Ok(Some(Self::Sparse(Arc::new(sparse))))
    }
}

impl TypeOp {
    fn parse(op_name: &str) -> Result<Self, DecodeError> {
        match op_name {
            "_array" => Ok(Self::Array),
            "_bitarray" => Ok(Self::BitArray),
            "_blob" => Ok(Self::Blob),
            "_bool" => Ok(Self::Bool),
            "_choice" => Ok(Self::Choice),
            "_fourcc" => Ok(Self::Fourcc),
            "_int" => Ok(Self::Int),
            "_null" => Ok(Self::Null),
            "_optional" => Ok(Self::Optional),
            "_real32" => Ok(Self::Real32),
            "_real64" => Ok(Self::Real64),
            "_struct" => Ok(Self::Struct),
            other => Err(DecodeError::Json(format!(
                "unsupported typeinfo opcode {other}"
            ))),
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Array => "_array",
            Self::BitArray => "_bitarray",
            Self::Blob => "_blob",
            Self::Bool => "_bool",
            Self::Choice => "_choice",
            Self::Fourcc => "_fourcc",
            Self::Int => "_int",
            Self::Null => "_null",
            Self::Optional => "_optional",
            Self::Real32 => "_real32",
            Self::Real64 => "_real64",
            Self::Struct => "_struct",
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct TypeInfo {
    typeid: usize,
    op: TypeOp,
    int_bounds: Option<IntBounds>,
    length_bounds: Option<IntBounds>,
    child_typeid: Option<usize>,
    choice_tag_bounds: Option<IntBounds>,
    choice_fields: Option<TagLookup<ChoiceField>>,
    struct_fields: Option<Arc<[StructField]>>,
    struct_fields_by_tag: Option<TagLookup<StructField>>,
}

impl TypeInfo {
    pub(crate) fn new(
        typeid: usize,
        op_name: &str,
        args: Vec<JsonValue>,
    ) -> Result<Self, DecodeError> {
        let op = TypeOp::parse(op_name)?;
        let int_bounds = if op == TypeOp::Int {
            Some(TypeInfoJsonParser::parse_int_bounds(
                args.first(),
                "_int bounds",
            )?)
        } else {
            None
        };
        let length_bounds = match op {
            TypeOp::Array | TypeOp::BitArray | TypeOp::Blob => Some(
                TypeInfoJsonParser::parse_int_bounds(args.first(), "length bounds")?,
            ),
            _ => None,
        };
        let child_typeid = match op {
            TypeOp::Array => Some(TypeInfoJsonParser::parse_typeid_arg(
                args.get(1),
                "_array typeid",
            )?),
            TypeOp::Optional => Some(TypeInfoJsonParser::parse_typeid_arg(
                args.first(),
                "_optional typeid",
            )?),
            _ => None,
        };
        let choice_tag_bounds = if op == TypeOp::Choice {
            Some(TypeInfoJsonParser::parse_int_bounds(
                args.first(),
                "_choice bounds",
            )?)
        } else {
            None
        };
        let choice_fields = if op == TypeOp::Choice {
            TypeInfoJsonParser::parse_choice_fields(&args)?
        } else {
            None
        };
        let struct_fields = if op == TypeOp::Struct {
            Some(Arc::from(TypeInfoJsonParser::parse_struct_fields(&args)?))
        } else {
            None
        };
        let struct_fields_by_tag = struct_fields
            .as_deref()
            .map(TypeInfoJsonParser::build_struct_field_tag_lookup)
            .transpose()?
            .flatten();

        Ok(Self {
            typeid,
            op,
            int_bounds,
            length_bounds,
            child_typeid,
            choice_tag_bounds,
            choice_fields,
            struct_fields,
            struct_fields_by_tag,
        })
    }

    pub(super) fn typeid(&self) -> usize {
        self.typeid
    }

    pub(super) fn op(&self) -> TypeOp {
        self.op
    }

    pub(super) fn op_name(&self) -> &'static str {
        self.op.as_str()
    }

    pub(super) fn int_bounds(&self) -> Result<IntBounds, DecodeError> {
        self.int_bounds
            .ok_or_else(|| DecodeError::Corrupted("_int bounds".into()))
    }

    pub(super) fn length_bounds(&self) -> Result<IntBounds, DecodeError> {
        self.length_bounds
            .ok_or_else(|| DecodeError::Corrupted("length bounds".into()))
    }

    pub(super) fn child_typeid(&self) -> Result<usize, DecodeError> {
        self.child_typeid
            .ok_or_else(|| DecodeError::Corrupted("child typeid".into()))
    }

    pub(super) fn choice_tag_bounds(&self) -> Result<IntBounds, DecodeError> {
        self.choice_tag_bounds
            .ok_or_else(|| DecodeError::Corrupted("_choice bounds".into()))
    }

    pub(super) fn choice_fields(&self) -> Result<&TagLookup<ChoiceField>, DecodeError> {
        self.choice_fields
            .as_ref()
            .ok_or_else(|| DecodeError::Corrupted("_choice map".into()))
    }

    pub(super) fn struct_fields(&self) -> Result<&[StructField], DecodeError> {
        self.struct_fields
            .as_deref()
            .ok_or_else(|| DecodeError::Corrupted("_struct fields".into()))
    }

    pub(super) fn struct_fields_by_tag(&self) -> Result<&TagLookup<StructField>, DecodeError> {
        self.struct_fields_by_tag
            .as_ref()
            .ok_or_else(|| DecodeError::Corrupted("_struct fields".into()))
    }
}

#[derive(Debug, Clone)]
pub(super) struct ChoiceField {
    name: Arc<str>,
    typeid: usize,
}

impl ChoiceField {
    fn new(name: String, typeid: usize) -> Self {
        Self {
            name: Arc::<str>::from(name),
            typeid,
        }
    }

    pub(super) fn name(&self) -> &str {
        self.name.as_ref()
    }

    pub(super) fn typeid(&self) -> usize {
        self.typeid
    }
}

#[derive(Debug, Clone)]
pub(super) struct StructField {
    name: Arc<str>,
    typeid: usize,
    tag: Option<i128>,
    is_parent: bool,
}

impl StructField {
    fn new(name: String, typeid: usize, tag: Option<i128>) -> Self {
        let is_parent = name == "__parent";
        Self {
            name: Arc::<str>::from(name),
            typeid,
            tag,
            is_parent,
        }
    }

    pub(super) fn name(&self) -> &str {
        self.name.as_ref()
    }

    pub(super) fn typeid(&self) -> usize {
        self.typeid
    }

    pub(super) fn is_parent(&self) -> bool {
        self.is_parent
    }

    pub(super) fn tag(&self) -> Option<i128> {
        self.tag
    }
}

struct TypeInfoJsonParser;

impl TypeInfoJsonParser {
    fn parse_choice_fields(
        args: &[JsonValue],
    ) -> Result<Option<TagLookup<ChoiceField>>, DecodeError> {
        let map = args
            .get(1)
            .and_then(JsonValue::as_object)
            .ok_or_else(|| DecodeError::Corrupted("_choice map".into()))?;

        let entries = map
            .iter()
            .map(|(tag, field)| -> Result<(i128, ChoiceField), DecodeError> {
                let parsed_tag = tag
                    .parse::<i128>()
                    .map_err(|_| DecodeError::Corrupted("_choice key".into()))?;
                let choice_field = Self::parse_choice_field(field)?;
                Ok((parsed_tag, choice_field))
            })
            .collect::<Result<Vec<_>, _>>()?;

        TagLookup::new(entries, "duplicate _choice tag")
    }

    fn parse_choice_field(value: &JsonValue) -> Result<ChoiceField, DecodeError> {
        let field = value
            .as_array()
            .ok_or_else(|| DecodeError::Corrupted("_choice value".into()))?;

        let field_name = field
            .first()
            .and_then(JsonValue::as_str)
            .ok_or_else(|| DecodeError::Corrupted("_choice field name".into()))?;
        let typeid = field
            .get(1)
            .and_then(JsonValue::as_u64)
            .ok_or_else(|| DecodeError::Corrupted("_choice field typeid".into()))?
            as usize;

        Ok(ChoiceField::new(field_name.to_string(), typeid))
    }

    fn parse_struct_fields(args: &[JsonValue]) -> Result<Vec<StructField>, DecodeError> {
        let fields = args
            .first()
            .and_then(JsonValue::as_array)
            .ok_or_else(|| DecodeError::Corrupted("_struct fields".into()))?;

        fields
            .iter()
            .map(Self::parse_struct_field)
            .collect::<Result<Vec<_>, _>>()
    }

    fn build_struct_field_tag_lookup(
        fields: &[StructField],
    ) -> Result<Option<TagLookup<StructField>>, DecodeError> {
        let entries = fields
            .iter()
            .filter_map(|field| field.tag().map(|tag| (tag, field.clone())))
            .collect::<Vec<_>>();

        TagLookup::new(entries, "duplicate _struct tag")
    }

    fn parse_struct_field(value: &JsonValue) -> Result<StructField, DecodeError> {
        let field = value
            .as_array()
            .ok_or_else(|| DecodeError::Corrupted("_struct field".into()))?;

        if field.len() < 2 {
            return Err(DecodeError::Corrupted("_struct field len".into()));
        }

        let field_name = field
            .first()
            .and_then(JsonValue::as_str)
            .ok_or_else(|| DecodeError::Corrupted("_struct field name".into()))?;
        let typeid = field
            .get(1)
            .and_then(JsonValue::as_u64)
            .ok_or_else(|| DecodeError::Corrupted("_struct field typeid".into()))?
            as usize;
        let tag = field.get(2).map(Self::json_to_i128).transpose()?;

        Ok(StructField::new(field_name.to_string(), typeid, tag))
    }

    fn json_to_i128(value: &JsonValue) -> Result<i128, DecodeError> {
        value
            .as_i64()
            .map(i128::from)
            .or_else(|| value.as_u64().map(i128::from))
            .ok_or_else(|| DecodeError::Corrupted("expected integer json value".into()))
    }

    fn parse_int_bounds(
        value: Option<&JsonValue>,
        context: &str,
    ) -> Result<IntBounds, DecodeError> {
        let bounds = value
            .and_then(JsonValue::as_array)
            .ok_or_else(|| DecodeError::Corrupted(context.into()))?;
        if bounds.len() != 2 {
            return Err(DecodeError::Corrupted(format!("{context} len")));
        }

        let min = bounds[0]
            .as_i64()
            .or_else(|| bounds[0].as_u64().map(|value| value as i64))
            .ok_or_else(|| DecodeError::Corrupted(format!("{context} min")))?;
        let bits = bounds[1]
            .as_u64()
            .and_then(|value| usize::try_from(value).ok())
            .ok_or_else(|| DecodeError::Corrupted(format!("{context} bits")))?;

        Ok(IntBounds { min, bits })
    }

    fn parse_typeid_arg(value: Option<&JsonValue>, context: &str) -> Result<usize, DecodeError> {
        value
            .and_then(JsonValue::as_u64)
            .and_then(|value| usize::try_from(value).ok())
            .ok_or_else(|| DecodeError::Corrupted(context.into()))
    }
}

#[derive(Debug, Clone)]
pub(crate) enum EventDecodePlan<F> {
    Ordered(Arc<[OrderedEventFieldPlan<F>]>),
    Tagged(Arc<TaggedEventDecodePlan<F>>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EventPlanKind {
    Ordered,
    Tagged,
}

#[derive(Debug, Clone)]
pub(crate) enum OrderedEventFieldPlan<F> {
    Decode { field: F, typeinfo: TypeInfo },
    Skip { typeinfo: TypeInfo },
    Nested(Arc<[OrderedEventFieldPlan<F>]>),
}

#[derive(Debug, Clone)]
pub(crate) struct TaggedEventDecodePlan<F> {
    fields_by_tag: TagLookup<TaggedEventFieldPlan<F>>,
}

impl<F> TaggedEventDecodePlan<F> {
    pub(super) fn new(fields_by_tag: TagLookup<TaggedEventFieldPlan<F>>) -> Self {
        Self { fields_by_tag }
    }

    pub(super) fn field_for_tag(&self, tag: &i128) -> Option<&TaggedEventFieldPlan<F>> {
        self.fields_by_tag.get(tag)
    }
}

#[derive(Debug, Clone)]
pub(crate) enum TaggedEventFieldPlan<F> {
    Decode { field: F, typeinfo: Box<TypeInfo> },
    Skip,
    Nested(Arc<TaggedEventDecodePlan<F>>),
}

#[derive(Debug, Clone)]
pub(crate) struct EventTypeInfo<F> {
    name: Arc<str>,
    decode_plan: Option<Arc<EventDecodePlan<F>>>,
}

impl<F> EventTypeInfo<F> {
    pub(crate) fn new(name: String, decode_plan: Option<EventDecodePlan<F>>) -> Self {
        Self {
            name: Arc::<str>::from(name),
            decode_plan: decode_plan.map(Arc::new),
        }
    }

    pub(super) fn name(&self) -> &str {
        self.name.as_ref()
    }

    pub(super) fn decode_plan(&self) -> Option<&EventDecodePlan<F>> {
        self.decode_plan.as_deref()
    }
}
