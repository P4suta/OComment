#[derive(Clone, Copy, Debug)]
pub(crate) struct Evidence {
    pub(crate) process: bool,
    pub(crate) completed: bool,
    pub(crate) identities: bool,
    pub(crate) version: bool,
    pub(crate) properties: bool,
    pub(crate) source: bool,
}

pub(crate) const fn admit(evidence: Evidence) -> bool {
    evidence.process
        && evidence.completed
        && evidence.identities
        && evidence.version
        && evidence.properties
        && evidence.source
}
