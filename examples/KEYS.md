# Example key material

These example configs expect the following private identity keys:

- `keys/gw-a.key` -> `MHrYFhY0MScnylFoN9mbonGR24UVhYwuRQCSNSS8mmg=`
- `keys/gw-b.key` -> `sG09sYd9gs6niI3tVRd16hkh7I/R3W3QFVmP7MMEk0g=`

The corresponding public keys are already embedded into the peer sections of the example configs.

Create them like this:

```bash
mkdir -p keys
printf '%s' 'MHrYFhY0MScnylFoN9mbonGR24UVhYwuRQCSNSS8mmg=' > keys/gw-a.key
printf '%s' 'sG09sYd9gs6niI3tVRd16hkh7I/R3W3QFVmP7MMEk0g=' > keys/gw-b.key
```
