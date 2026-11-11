# Files and Uploads Resource Proposals

This document details proposed designs for OpenAI-compatible File management (`/v1/files`) and Multipart Uploads (`/v1/uploads`).

See [Proposals Framework](index.md) for shared boundaries and storage lifecycle review.

## Contents

- [Files and uploads](#files-and-uploads)

## Files and uploads

Files/multipart Uploads are selected for stage 4. They need typed resource operations, not an inference request or a larger Generation resource table.

| Method and path | Proposed operation |
|---|---|
| `POST /v1/files` | Create a file |
| `GET /v1/files` | List accessible files |
| `GET /v1/files/{file_id}` | Retrieve metadata |
| `GET /v1/files/{file_id}/content` | Read content |
| `DELETE /v1/files/{file_id}` | Delete a file |
| `POST /v1/uploads` | Create a bounded multipart upload |
| `POST /v1/uploads/{upload_id}/parts` | Add a part |
| `POST /v1/uploads/{upload_id}/complete` | Complete ordered parts into a usable file |
| `POST /v1/uploads/{upload_id}/cancel` | Cancel an incomplete upload |

The resource service owns public identities, access checks, metadata, purpose, retention, upload state and private upstream mappings. The conversation resource table remains request-local content ownership.

Choose authoritative storage before implementation. The initial Files-slice recommendation is upstream-native content storage with gateway-owned public IDs and durable private mappings. The alternative is gateway-owned content with per-target upstream replicas. Neither is selected yet.

Define finite storage/upload bounds, part order and byte-total validation, incomplete-upload cleanup and supported purposes. Do not adopt deprecated File status fields as a new processing state machine. Accepting a file purpose does not implement its associated fine-tuning or agent service.

Resolve resource authorization outside pure codecs. Task content may carry typed resource identity, but credentials, storage handles and trusted target mappings stay outside IR. Resource scope can restrict already authorized targets; it cannot authorize an arbitrary origin or credential.

Exit evidence: create/read/list/delete and upload lifecycle contracts, restart-consistent mappings, access isolation, expiry, deletion with active references, and bounded streaming rather than whole-file memory retention.

