#[derive(Debug, Clone)]
pub(crate) struct LiveGamePlayer {
    pub(crate) id: u64,
    pub(crate) name: String,
    pub(crate) kind: String,
    pub(crate) handle: String,
}
