//! Bounded JSON snapshots, including projection of an SSE upstream into JSON.
use super::*;
use crate::execution::report::EncodedBatch;
use bytes::Bytes;

impl ResponseDelivery {
    pub fn encode_json(&mut self, attempt: &Attempt) -> Result<Vec<u8>, AttemptError> {
        self.encode_json_snapshot(attempt).map(|(_, batch)| {
            batch
                .into_bytes()
                .into_iter()
                .flat_map(|b| b.to_vec())
                .collect()
        })
    }
    /// Finalize public JSON and retain its typed history, including managed-only replay.
    pub fn encode_json_snapshot(
        &mut self,
        attempt: &Attempt,
    ) -> Result<(crate::protocol::DecodedResponse, EncodedBatch), AttemptError> {
        let result = (|| {
            self.ready()?;
            if matches!(self.output, OutputMode::Sse(_)) {
                return Err(AttemptError::Delivery(
                    crate::execution::attempt::DeliveryReason::StreamAlreadyStarted,
                ));
            }
            self.bind(attempt)?;
            let (response, bytes, evidence) =
                self.checked_snapshot(attempt, View::DownstreamJson)?;
            self.projection.rendered = true;
            self.projection.phase = ProjectionPhase::Finalized;
            let batch = self
                .report
                .batch(View::DownstreamJson, vec![Bytes::from(bytes)], evidence);
            Ok((response, batch))
        })();
        if result.is_err() {
            self.projection.phase = ProjectionPhase::Rejected;
        }
        result
    }
    #[cfg(test)]
    pub fn snapshot(
        &mut self,
        attempt: &Attempt,
    ) -> Result<(crate::protocol::DecodedResponse, Vec<u8>), AttemptError> {
        let (response, bytes, evidence) = self.checked_snapshot(attempt, View::RetainedJson)?;
        self.report.accept(View::RetainedJson, evidence);
        Ok((response, bytes))
    }
    fn checked_snapshot(
        &mut self,
        attempt: &Attempt,
        view: View,
    ) -> Result<(crate::protocol::DecodedResponse, Vec<u8>, EvidenceDelta), AttemptError> {
        let mut evidence = self.complete_presentation(attempt, view)?;
        let mut response = self.client_response(attempt)?;
        let source = response.clone();
        if source
            .semantic
            .items()
            .iter()
            .any(|(_, item)| item.has_replay_attachment())
        {
            return Err(CodecError::Unsupported(CarrierReason::Replay).into());
        }
        if !self
            .projection
            .adapter
            .as_ref()
            .is_some_and(|projection| projection.started())
        {
            let wire = crate::protocol::replay_projection::strip(&response, false)?;
            self.project_events(attempt, &[], false)?;
            let projection = self.projection.adapter.as_mut().expect("projection");
            projection.update_fidelity(&wire.fidelity)?;
            projection.metadata(&wire.metadata)?;
            crate::adapter::emit_response_events(&wire, |event| {
                projection.project(&[event]).map(|_| ())
            })?;
            projection.update_fidelity(&wire.fidelity)?;
        } else if !self
            .projection
            .adapter
            .as_ref()
            .expect("projection")
            .finished()
        {
            self.projection
                .adapter
                .as_mut()
                .expect("projection")
                .metadata(&response.metadata)?;
            self.project_events(attempt, &[attempt.terminal_event()?.clone()], false)?;
        }
        self.projection
            .adapter
            .as_mut()
            .expect("projection")
            .update_fidelity(&source.fidelity)?;
        response.semantic = self
            .projection
            .adapter
            .as_ref()
            .expect("projection")
            .snapshot()?;
        response.metadata = self
            .projection
            .adapter
            .as_mut()
            .expect("projection")
            .snapshot_metadata(&response.metadata)?;
        response.fidelity = self
            .projection
            .adapter
            .as_ref()
            .expect("projection")
            .fidelity()
            .clone();
        response.fidelity.retain_owners(response.semantic.items());
        evidence.extend(
            self.projection
                .adapter
                .as_mut()
                .expect("projection")
                .take_evidence(),
        );
        let (decoded, mut value) = self.client.render_projected(response)?;
        if let Some(notice) = self.report.preview_notice(view, &evidence) {
            notice.attach(&mut value, self.limits.max_wire_bytes, false);
        }
        let body = serde_json::to_vec(&value).map_err(|_| {
            AttemptError::Protocol(crate::execution::attempt::ProtocolReason::BodySerialization)
        })?;
        if body.len() > self.limits.max_wire_bytes {
            return Err(AttemptError::Limit);
        }
        Ok((decoded, body, evidence))
    }
    pub fn observe_events(
        &mut self,
        attempt: &Attempt,
        events: &[StreamEvent],
    ) -> Result<(), AttemptError> {
        self.ready()?;
        if matches!(self.output, OutputMode::Sse(_)) {
            return Err(AttemptError::Delivery(
                crate::execution::attempt::DeliveryReason::StreamAlreadyStarted,
            ));
        }
        self.bind(attempt)?;
        let fidelity = attempt.fidelity().ok_or(AttemptError::Protocol(
            crate::execution::attempt::ProtocolReason::MissingResponseFidelity,
        ))?;
        let events = events
            .iter()
            .filter_map(|event| crate::protocol::replay_projection::wire_event(event, fidelity))
            .map(|event| {
                super::super::replay::seal_event(
                    event,
                    &crate::protocol::replay_envelope::CLIENT_ORIGIN,
                    fidelity,
                )
            })
            .collect::<Result<Vec<_>, _>>()?;
        self.project_events(attempt, &[], false)?;
        let metadata = self.metadata(attempt)?;
        let projection = self.projection.adapter.as_mut().expect("projection");
        projection.update_fidelity(fidelity)?;
        projection.metadata(&metadata)?;
        self.project_events(attempt, &events, false)?;
        Ok(())
    }
}
