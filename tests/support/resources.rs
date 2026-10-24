//! Synthetic declarations; expected wire values stay in the owning tests.
use morphiecore::semantic::task::generation::*;
pub fn image(id: u64) -> ResourceUse {
    ResourceUse {
        id: ResourceId::new(id),
        description: ResourceDescription::Image {
            detail: None,
            size_policy: ImageSizePolicy::AllowDownsize,
        },
    }
}
pub fn replace(table: &ResourceTable, id: ResourceId, body: Resource) -> ResourceTable {
    let mut entries: Vec<_> = table
        .iter()
        .filter(|(key, _)| *key != id)
        .map(|(key, value)| (key, value.clone()))
        .collect();
    entries.push((id, body.location));
    ResourceTable::new(entries).unwrap()
}
