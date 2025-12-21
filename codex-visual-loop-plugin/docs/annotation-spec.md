# Annotation Spec

`annotate_image.py` supports:

- Relative units via `defaults.units: "rel"`
- Semantic fields per annotation:
  - `severity`
  - `issue`
  - `hypothesis`
  - `next_action`
  - `verify`

These semantic fields are preserved in output metadata sidecars.
