# Sensors

Weathervane reads four sensors over I²C and one on a
GPIO pin. Each has a driver in `src/sensors/`, polled on
its own schedule, so a slow sensor never holds up a
fast one.

> **Tip:** run `weathervane probe` after wiring a new
> sensor. It lists every address on the bus.

## What's supported

| Sensor | Measures | Bus |
| --- | --- | --- |
| BME280 | Temperature, humidity, pressure | I²C `0x76` |
| SHT45 | Temperature, humidity | I²C `0x44` |
| VEML7700 | Ambient light | I²C `0x10` |
| AS3935 | Lightning distance | I²C `0x03` |
| Anemometer | Wind speed | GPIO 17 |

## Wiring

Chain the I²C sensors together: SDA to GPIO 2, SCL to
GPIO 3, and power each from the 3.3 V rail. Keep the
leads under **30 cm**.

1. Power the board down first.
2. Wire the BME280, then run `weathervane probe`.
3. Add the others one at a time.

- [x] BME280 on the north wall
- [x] Anemometer on the mast
- [ ] Lightning sensor, once the enclosure arrives

## Calibration

Pressure depends on altitude. Set the station's
elevation once, and every reading is corrected:

```toml
[station]
name = "Back garden"
elevation_m = 212

[sensors.bme280]
address = 0x76
offset_c = -0.4  # the case warms it
```

## Adding a driver

A driver implements one trait, returning a `Reading`
for each quantity it measures:

```rust
impl Sensor for Veml7700 {
    fn read(&mut self, bus: &mut I2c) -> Result<Vec<Reading>> {
        let lux = self.lux(bus)?;
        Ok(vec![Reading::new(Quantity::Light, lux)])
    }
}
```

See [Configuration](configuration.md) for every setting.

## Troubleshooting

A sensor that reads `NaN` usually shares its address
with another board. The [field log](../notes/field-log.md)
has the fixes we've needed so far, and the README says
[what each sensor records](../README.md#what-it-records).
