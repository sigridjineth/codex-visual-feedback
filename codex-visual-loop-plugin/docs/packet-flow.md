# Observation Packet Flow

`observe_action_clip.sh` emits one JSON packet that includes:

- `before_capture`
- `after_capture`
- `clip`
- `diff` (with change-region boxes and optional annotation spec/artifacts)
- action metadata (`label`, `command`, `status`, timestamps, log path)
