# Weathervane

A small weather station for the back garden: a Raspberry Pi, five sensors on a mast, and a
dashboard that keeps a year of readings.

## What it records

- Temperature, humidity and pressure, every ten seconds
- Wind speed and direction
- Ambient light, for sunrise, sunset and cloud cover
- Lightning strikes within 40 km

## Getting started

Follow [Getting started](docs/getting-started.md) to flash the card and wire the first
sensor, then [Sensors](docs/sensors.md) for the rest.

```sh
weathervane probe
weathervane run --config station.toml
```

## Documentation

| Guide | What it covers |
| --- | --- |
| [Getting started](docs/getting-started.md) | The board, the card and the first reading |
| [Sensors](docs/sensors.md) | Wiring, calibration and drivers |
| [Configuration](docs/configuration.md) | Every setting in `station.toml` |
| [Dashboard](docs/dashboard.md) | Charts, alerts and exports |
