# RFC-001: Event Model

Events represent any distinct action matching standard schema.

Fields:
- `id`: UUID
- `timestamp`: UTC datetime
- `event_type`: string representing classification
- `project_id`: string
- `payload`: JSON
