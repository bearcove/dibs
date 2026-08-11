# dibs-runtime

Runtime support for dibs-generated PostgreSQL queries:

- `many`, `optional`, `one`, and `exec` result-mode helpers
- typed `UnexpectedRowCount` failures without SQL truncation
- minimal `QueryContext` identity and preserved PostgreSQL/decode errors
- duration plus rows-or-affected tracing with no bind values

Part of the [dibs](https://github.com/facet-rs/facet) project.
