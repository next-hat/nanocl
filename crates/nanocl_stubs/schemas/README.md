# Bundled cloud-init schema

`cloud-init-26.2.json` is the unmodified official cloud-config schema from
[cloud-init 26.2](https://github.com/canonical/cloud-init/blob/26.2/cloudinit/config/schemas/schema-cloud-config-v1.json).

- Upstream Git blob: `7796073c9281fffd669f5efa874aa1f55ee95fbb`
- SHA-256: `a2923d8886201b5478683cf9c6118a00d5603f36d69f8ad899c5b7909c807d63`
- Copyright Canonical Ltd.; distributed here under the Apache License 2.0
  option in the accompanying, unchanged upstream `LICENSE-cloud-init` notice.
  The full Apache License text is in the package-local [LICENSE-APACHE](LICENSE-APACHE),
  so published `nanocl_stubs` archives include both the upstream attribution and
  the selected license. The upstream notice also offers GPLv3; that alternative
  is not the license selected for this bundled copy. This third-party asset is
  not covered by NanoCL's MIT option.

`vm_spec.rs` registers the root as `CloudInit26_2` and the upstream `$defs`
as `CloudInit26_2.<name>`, rewriting references to the containing document's
local schema definitions. It removes the embedded `$schema` declaration and
adds `type: object` to the root to match NanoCL's structured user-data type.
The export also corrects one invalid upstream wrapper at
`$defs.modules_definition.items.oneOf[1].prefixItems[0]`: it replaces
`{"enum":{"$ref":"#/$defs/all_modules"}}` with a direct `$ref` to the same
module-name enum. JSON Schema requires an array for `enum`; this correction
retains the upstream module-name constraints. The bundled JSON remains unchanged.
All other upstream keywords are preserved, including nested validation rules.
OpenAPI uses local component references so NanoCL's documentation and VS Code
Statefile consumers work without fetching external schemas.

When updating the pinned version, replace the source file and update this
provenance, the URL/name constants, and the schema tests. Check both OpenAPI
and Schemars output and run the documentation/VS Code schema consumers.
The bundled schema describes cloud-config; the daemon does not perform guest
module validation, and the installed guest cloud-init version controls support.
