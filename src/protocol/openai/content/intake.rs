//! Shared item/resource construction during OpenAI content intake.
use crate::protocol::openai::{CodecError, shape::DecodeDirection};
use crate::{
    protocol::fidelity::FidelityRecords,
    semantic::{task::generation::*, value::Text},
};
use serde_json::{Map, Value};

#[derive(Default)]
pub(in crate::protocol::openai) struct Items<'a> {
    pub direction: DecodeDirection<'a>,
    pub items: Vec<(ItemId, Item)>,
    pub resources: ResourceTable,
    pub message_owners: Vec<(ItemId, ItemId)>,
    pub fidelity: FidelityRecords,
    pub attachments: Vec<crate::protocol::attachments::Marker>,
    scope: LocalScope,
    next_item: u64,
    next_part: u64,
    next_resource: u64,
}
impl<'a> Items<'a> {
    pub fn new(direction: DecodeDirection<'a>) -> Self {
        Self::in_scope(direction, LocalScope::ROOT)
    }
    pub fn in_scope(direction: DecodeDirection<'a>, scope: LocalScope) -> Self {
        Self {
            direction,
            scope,
            ..Default::default()
        }
    }
    pub fn resource(&mut self, value: Resource) -> Result<ResourceUse, CodecError> {
        Ok(ResourceUse {
            id: self.declare(value.location)?,
            description: value.description,
        })
    }
    pub fn from_resources(resources: ResourceTable, direction: DecodeDirection<'a>) -> Self {
        let next_resource = resources.iter().map(|(id, _)| id.get()).max().unwrap_or(0);
        Self {
            resources,
            direction,
            next_resource,
            ..Default::default()
        }
    }
    pub fn declare(&mut self, value: ResourceLocation) -> Result<ResourceId, CodecError> {
        if self.next_resource as usize >= MAX_ITEMS {
            return Err(CodecError::Limit);
        }
        self.next_resource += 1;
        let id = ResourceId::scoped(self.scope, self.next_resource);
        self.resources = std::mem::take(&mut self.resources).insert(id, value)?;
        Ok(id)
    }
    pub fn id(&mut self) -> Result<ItemId, CodecError> {
        if self.next_item as usize >= MAX_ITEMS {
            return Err(CodecError::Limit);
        }
        self.next_item += 1;
        Ok(ItemId::scoped(self.scope, self.next_item))
    }
    pub fn part_id(&mut self) -> Result<PartId, CodecError> {
        self.next_part += 1;
        if self.next_part as usize > MAX_ITEMS {
            return Err(CodecError::Limit);
        }
        Ok(PartId::scoped(self.scope, self.next_part))
    }
    pub fn refusal(&mut self, text: Text) -> Result<Part, CodecError> {
        self.next_part += 1;
        if self.next_part as usize > MAX_ITEMS {
            return Err(CodecError::Limit);
        }
        Ok(Part {
            replay: None,
            id: PartId::scoped(self.scope, self.next_part),
            content: ContentPart::Refusal(text.into()),
            cache: None,
        })
    }
    pub fn part(&mut self, s: &str) -> Result<Part, CodecError> {
        self.next_part += 1;
        if self.next_part as usize > MAX_ITEMS {
            return Err(CodecError::Limit);
        }
        Ok(Part {
            replay: None,
            id: PartId::scoped(self.scope, self.next_part),
            cache: None,
            content: ContentPart::Text(
                Text::allowing_empty(s, "text", MAX_TEXT_BYTES)
                    .map_err(|_| CodecError::Limit)?
                    .into(),
            ),
        })
    }
    pub fn record_id(&mut self, id: ItemId, o: &Map<String, Value>) -> Result<(), CodecError> {
        if let Some(v) = o.get("id") {
            self.fidelity
                .record_response_item_id(id, v.as_str().ok_or(CodecError::Invalid("item id"))?)?;
        }
        Ok(())
    }
}
