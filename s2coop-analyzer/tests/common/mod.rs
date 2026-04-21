use s2coop_analyzer::detailed_replay_analysis::ReplayAnalysisResources;
use s2coop_analyzer::dictionary_data::Sc2DictionaryData;
use std::sync::Arc;

pub fn load_dictionary() -> Sc2DictionaryData {
    Sc2DictionaryData::load(None).expect("dictionary data should load for tests")
}

pub fn load_replay_resources() -> ReplayAnalysisResources {
    ReplayAnalysisResources::from_dictionary_data(Arc::new(load_dictionary()))
        .expect("replay analysis resources should load for tests")
}
