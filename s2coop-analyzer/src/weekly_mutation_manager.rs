use crate::dictionary_data::{Sc2DictionaryData, WeeklyMutationDateJson, WeeklyMutationsJson};
use chrono::{Duration, Local, NaiveDate};
use thiserror::Error;

const DAYS_PER_MUTATION: i64 = 7;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WeeklyMutationStatus {
    pub name: String,
    pub order: usize,
    pub is_current: bool,
    pub next_start_date: NaiveDate,
    pub next_duration_days: i64,
}

#[derive(Clone, Debug, Error)]
pub enum WeeklyMutationManagerError {
    #[error("weekly mutation list is empty")]
    EmptyMutationList,
    #[error("initial weekly mutation '{0}' was not found")]
    InitialMutationNotFound(String),
    #[error("initial weekly mutation date '{0}' must use YYYY-MM-DD format")]
    InvalidInitialDate(String),
}

#[derive(Clone, Debug)]
pub struct WeeklyMutationManager {
    weekly_mutations: WeeklyMutationsJson,
    initial_name: String,
    initial_date: NaiveDate,
}

impl WeeklyMutationManager {
    pub fn new(
        weekly_mutations: &WeeklyMutationsJson,
        initial: &WeeklyMutationDateJson,
    ) -> Result<Self, WeeklyMutationManagerError> {
        if weekly_mutations.is_empty() {
            return Err(WeeklyMutationManagerError::EmptyMutationList);
        }

        if !weekly_mutations.contains_key(&initial.name) {
            return Err(WeeklyMutationManagerError::InitialMutationNotFound(
                initial.name.clone(),
            ));
        }

        let initial_date = NaiveDate::parse_from_str(&initial.date, "%Y-%m-%d")
            .map_err(|_| WeeklyMutationManagerError::InvalidInitialDate(initial.date.clone()))?;

        Ok(Self {
            weekly_mutations: weekly_mutations.clone(),
            initial_name: initial.name.clone(),
            initial_date,
        })
    }

    pub fn from_dictionary_data(
        dictionary_data: &Sc2DictionaryData,
    ) -> Result<WeeklyMutationManager, WeeklyMutationManagerError> {
        WeeklyMutationManager::new(
            &dictionary_data.weekly_mutations_json,
            &dictionary_data.weekly_mutation_date_json,
        )
    }

    pub fn current(&self) -> Result<WeeklyMutationStatus, WeeklyMutationManagerError> {
        self.current_for_date(Local::now().date_naive())
    }

    pub fn current_for_date(
        &self,
        date: NaiveDate,
    ) -> Result<WeeklyMutationStatus, WeeklyMutationManagerError> {
        let current_index = self.current_index_for_date(date)?;
        let current_start_date = self.current_start_date_for(date);
        let current_name = self
            .weekly_mutations
            .keys()
            .nth(current_index)
            .cloned()
            .ok_or(WeeklyMutationManagerError::EmptyMutationList)?;

        Ok(WeeklyMutationStatus {
            name: current_name,
            order: current_index,
            is_current: true,
            next_start_date: current_start_date,
            next_duration_days: 0,
        })
    }

    pub fn statuses_for_date(
        &self,
        date: NaiveDate,
    ) -> Result<Vec<WeeklyMutationStatus>, WeeklyMutationManagerError> {
        let total = self.weekly_mutations.len();
        if total == 0 {
            return Err(WeeklyMutationManagerError::EmptyMutationList);
        }

        let current_index = self.current_index_for_date(date)?;
        let current_start_date = self.current_start_date_for(date);
        let total_i64 = i64::try_from(total).unwrap_or(0);

        let mut statuses = Vec::with_capacity(total);
        for (index, name) in self.weekly_mutations.keys().enumerate() {
            let weeks_until = (i64::try_from(index).unwrap_or(0)
                - i64::try_from(current_index).unwrap_or(0))
            .rem_euclid(total_i64);
            let is_current = index == current_index;
            let next_start_date =
                current_start_date + Duration::days(weeks_until * DAYS_PER_MUTATION);
            statuses.push(WeeklyMutationStatus {
                name: name.clone(),
                order: index,
                is_current,
                next_start_date,
                next_duration_days: if is_current {
                    0
                } else {
                    (next_start_date - date).num_days()
                },
            });
        }

        Ok(statuses)
    }

    fn current_index_for_date(&self, date: NaiveDate) -> Result<usize, WeeklyMutationManagerError> {
        let total = self.weekly_mutations.len();
        if total == 0 {
            return Err(WeeklyMutationManagerError::EmptyMutationList);
        }

        let start_index = self
            .weekly_mutations
            .keys()
            .position(|name| name == &self.initial_name)
            .ok_or_else(|| {
                WeeklyMutationManagerError::InitialMutationNotFound(self.initial_name.to_string())
            })?;

        let days_since_start = (date - self.initial_date).num_days();
        let weeks_since_start = days_since_start.div_euclid(DAYS_PER_MUTATION);
        let total_i64 = i64::try_from(total).unwrap_or(1);
        let current_index =
            (i64::try_from(start_index).unwrap_or(0) + weeks_since_start).rem_euclid(total_i64);
        usize::try_from(current_index).map_err(|_| WeeklyMutationManagerError::EmptyMutationList)
    }

    fn current_start_date_for(&self, date: NaiveDate) -> NaiveDate {
        let days_since_start = (date - self.initial_date).num_days();
        let weeks_since_start = days_since_start.div_euclid(DAYS_PER_MUTATION);
        self.initial_date + Duration::days(weeks_since_start * DAYS_PER_MUTATION)
    }
}
