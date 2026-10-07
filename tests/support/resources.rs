//! Synthetic declarations; expected wire values stay in the owning tests.
use morphiecore::semantic::task::generation::*;
pub fn table(entries: impl IntoIterator<Item = (u64, Resource)>) -> ResourceTable {
    ResourceTable::new(
        entries
            .into_iter()
            .map(|(id, value)| {
                (
                    ResourceId::new(id),
                    ResourceDeclaration {
                        body: ResourceBody::Media(value.location),
                        conditions: Default::default(),
                    },
                )
            })
            .collect(),
    )
    .unwrap()
}
pub fn input(id: u64) -> ResourceUse {
    ResourceUse {
        id: ResourceId::new(id),
        purpose: ResourcePurpose::Input,
        description: ResourceDescription::Image { detail: None },
    }
}
pub fn output(id: u64) -> ResourceUse {
    ResourceUse {
        id: ResourceId::new(id),
        purpose: ResourcePurpose::Output,
        description: ResourceDescription::Image { detail: None },
    }
}
pub fn tool(id: u64) -> ResourceUse {
    ResourceUse {
        id: ResourceId::new(id),
        purpose: ResourcePurpose::ToolResult,
        description: ResourceDescription::Image { detail: None },
    }
}
pub fn replace(table: &ResourceTable, id: ResourceId, body: Resource) -> ResourceTable {
    let mut entries: Vec<_> = table
        .iter()
        .filter(|(key, _)| *key != id)
        .map(|(key, value)| (key, value.clone()))
        .collect();
    entries.push((
        id,
        ResourceDeclaration {
            body: ResourceBody::Media(body.location),
            conditions: Default::default(),
        },
    ));
    ResourceTable::new(entries).unwrap()
}
