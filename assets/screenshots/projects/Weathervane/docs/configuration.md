# Configuration

Everything lives in `station.toml`, beside the program. Weathervane reads it at start and
again whenever it changes.

## Station

| Key | Default | Meaning |
| --- | --- | --- |
| `name` | `"Weathervane"` | Shown on the dashboard |
| `elevation_m` | `0` | For the pressure correction |
| `units` | `"metric"` | Or `"imperial"` |

## Sensors

Each sensor has a table of its own, named for its driver. Every table takes `address`,
`interval_s` and `offset`.

## Storage

Readings go to a SQLite file. A year at the default intervals is about 180 MB.
