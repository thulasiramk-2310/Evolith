# RFC-001: Event Model

Events represent any distinct action matching standard schema.

Fields:
- id: UUID
- 	imestamp: UTC datetime
- event_type: string representing classification
- project_id: string
- payload: JSON
- path: string (optional)
- ranch: string (optional)
- metadata: JSON (optional)
- collector_version: string
- created_at: UTC datetime
