use std::collections::HashSet;

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct UnitCost {
    pub(crate) mineral: f64,
    pub(crate) gas: f64,
}

impl UnitCost {
    pub(crate) fn zero() -> Self {
        Self {
            mineral: 0.0,
            gas: 0.0,
        }
    }

    pub(crate) fn new(mineral: f64, gas: f64) -> Self {
        Self { mineral, gas }
    }

    pub(crate) fn sum(self) -> f64 {
        self.mineral + self.gas
    }

    pub(crate) fn scaled(self, min_mult: f64, gas_mult: f64) -> Self {
        Self {
            mineral: self.mineral * min_mult,
            gas: self.gas * gas_mult,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub(crate) struct TotalUnitCost {
    values: Vec<UnitCost>,
}

impl TotalUnitCost {
    pub(crate) fn zero() -> Self {
        Self {
            values: vec![UnitCost::zero()],
        }
    }

    pub(crate) fn from_slice(values: &[f64]) -> Self {
        let values = values
            .chunks_exact(2)
            .map(|chunk| UnitCost::new(chunk[0], chunk[1]))
            .collect::<Vec<UnitCost>>();
        if values.is_empty() {
            Self::zero()
        } else {
            Self { values }
        }
    }

    pub(crate) fn len(&self) -> usize {
        self.values.len()
    }

    pub(crate) fn first(&self) -> UnitCost {
        self.values.first().copied().unwrap_or_else(UnitCost::zero)
    }

    pub(crate) fn get(&self, index: usize) -> UnitCost {
        self.values
            .get(index)
            .copied()
            .unwrap_or_else(UnitCost::zero)
    }

    pub(crate) fn sum(&self) -> f64 {
        self.values.iter().map(|value| value.sum()).sum()
    }

    pub(crate) fn scaled(&self, min_mult: f64, gas_mult: f64) -> Self {
        Self {
            values: self
                .values
                .iter()
                .copied()
                .map(|value| value.scaled(min_mult, gas_mult))
                .collect(),
        }
    }

    pub(crate) fn scaled_mineral(&self, min_mult: f64) -> Self {
        self.scaled(min_mult, 1.0)
    }

    pub(crate) fn scaled_gas(&self, gas_mult: f64) -> Self {
        self.scaled(1.0, gas_mult)
    }
}

pub(crate) struct StatsCounterMath;

impl StatsCounterMath {
    pub(crate) fn normalize_commander_name(commander: &str) -> String {
        if commander == "Han & Horner" {
            "Horner".to_string()
        } else {
            commander.to_string()
        }
    }

    pub(crate) fn remove_upward_spikes(values: &mut [f64]) {
        if values.len() < 3 {
            return;
        }
        for idx in 1..(values.len() - 1) {
            if values[idx] > values[idx - 1] && values[idx] > values[idx + 1] {
                values[idx] = (values[idx - 1] + values[idx + 1]) / 2.0;
            }
        }
    }

    pub(crate) fn upward_spike_indices(values: &[f64]) -> HashSet<usize> {
        let mut indices = HashSet::new();
        if values.len() < 3 {
            return indices;
        }
        for idx in 1..(values.len() - 1) {
            if values[idx] > values[idx - 1] && values[idx] > values[idx + 1] {
                indices.insert(idx);
            }
        }
        indices
    }

    pub(crate) fn rolling_average(values: &[f64]) -> Vec<f64> {
        if values.is_empty() {
            return Vec::new();
        }

        let mut out = Vec::with_capacity(values.len());
        for (idx, value) in values.iter().enumerate() {
            if idx == 0 {
                out.push(*value);
            } else {
                out.push(0.5 * *value + 0.5 * values[idx - 1]);
            }
        }
        out
    }
}
