//! Transfer verified owner-local records across a private history identity remap.
use super::*;

impl FidelityRecords {
    pub(crate) fn import_history_item(
        &mut self,
        source: &Self,
        old_id: ItemId,
        old: &Item,
        new_id: ItemId,
        new: &Item,
    ) -> Result<(), CodecError> {
        if let Some(id) = source.response_item_id(old_id) {
            self.record_response_item_id(new_id, id)?;
        }
        if let Some(binding) = source.item_replay.get(&old_id) {
            let dependency = match old {
                Item::Reasoning(r) if binding.kind == ReplayOwnerKind::Reasoning => {
                    replay::fingerprint(r)
                }
                Item::ToolCall(c) if binding.kind == ReplayOwnerKind::Call => {
                    replay::call_dependency(c)?
                }
                Item::Compaction(value) if binding.kind == ReplayOwnerKind::Compaction => {
                    value.fingerprint()
                }
                _ => {
                    return Err(CodecError::Invalid(
                        crate::protocol::failure::ShapeReason::InvalidValue,
                    ));
                }
            };
            if binding.dependency != dependency {
                return Err(CodecError::Invalid(
                    crate::protocol::failure::ShapeReason::InvalidValue,
                ));
            }
            match new {
                Item::Reasoning(r) => self.record_replay(new_id, r, binding.origin.clone())?,
                Item::ToolCall(c) => self.record_call_replay(new_id, c, binding.origin.clone())?,
                Item::Compaction(value) => {
                    self.record_compaction(new_id, value, binding.origin.clone())?
                }
                _ => {
                    return Err(CodecError::Invalid(
                        crate::protocol::failure::ShapeReason::InvalidValue,
                    ));
                }
            }
        }
        // Creator and response-wide reports are output observations, not request
        // history. In particular, sealing may invalidate an old creator receipt.
        let old_parts: Vec<_> = old.part_ids().collect();
        let new_parts: Vec<_> = new.part_ids().collect();
        for (a, b) in old_parts.into_iter().zip(new_parts) {
            if source.input_text_form(a) {
                self.record_input_text(b)?;
            }
        }
        if let (Item::Message(old), Item::Message(new)) = (old, new) {
            for (ordinal, (a, b)) in old.parts.iter().zip(&new.parts).enumerate() {
                if let Some(binding) = source.part_replay.get(&(old_id, a.id)) {
                    if binding.dependency != replay::part_dependency(old.status, ordinal, a)? {
                        return Err(CodecError::Invalid(
                            crate::protocol::failure::ShapeReason::InvalidValue,
                        ));
                    }
                    self.record_part_replay(
                        new_id,
                        new.status,
                        ordinal,
                        b,
                        binding.origin.clone(),
                    )?;
                }
                if let Some(binding) = source.audio.get(&a.id) {
                    let (reference, body) = match &a.content {
                        ContentPart::Audio(v) => {
                            (v.reference().fingerprint(), Some(v.fingerprint()))
                        }
                        ContentPart::AudioReference(v) => (v.fingerprint(), None),
                        _ => {
                            return Err(CodecError::Invalid(
                                crate::protocol::failure::ShapeReason::InvalidValue,
                            ));
                        }
                    };
                    if binding.reference != reference || body.is_some() && binding.body != body {
                        return Err(CodecError::Invalid(
                            crate::protocol::failure::ShapeReason::InvalidValue,
                        ));
                    }
                    self.record_audio(b.id, &b.content, binding.origin.clone())?;
                }
            }
        }
        Ok(())
    }
}
