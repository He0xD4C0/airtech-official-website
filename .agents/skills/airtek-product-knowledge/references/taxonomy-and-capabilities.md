# Taxonomy and capabilities

Last reviewed: 2026-09-14

## Product families

The supplied company materials support this family-level structure:

1. Centrifugal fans
   - backward-curved
   - forward-curved
2. Axial fans
3. Cross-flow fans
4. Inline duct fans
5. Motors
   - AC, DC, and EC external-rotor motors
   - air-conditioning motors
   - shaded-pole motors
   - gas-blower motors
   - electronics motors
6. Customized ventilation solutions and OEM/ODM engineering

Status: `VERIFIED` as a normalized family taxonomy from the supplied company deck PDF pp. 13–18. Confirm names, slugs, model membership, and commercial availability against Product Master before launch.

Do not put `EC Fans` beside centrifugal/axial/cross-flow/inline as the only flat hierarchy. EC is primarily a motor/commutation technology and should normally be a facet that can apply across compatible fan forms. Similarly, single/double inlet and forward/backward-curved are properties or subtypes, not universal peer categories.

## Application domains

The materials associate AIRTEKPOWER products or engineering with:

- HVAC and air-handling units.
- Refrigeration and heating.
- Data centers and fan-grid systems.
- Energy storage and charging infrastructure.
- Air purification, cleanrooms, and controlled environments.
- Electronics and ICT cooling.
- Industrial ventilation and exhaust.
- Rail transit.
- Medical equipment.
- Home appliances.
- Mechanical equipment.
- Heat exchangers and convectors.
- Bus and recreational-vehicle air conditioning.

Status: `PROVISIONAL` application coverage reported in the supplied company deck PDF pp. 3 and 19–26. It may organize content discovery, but it is not proof that every domain is currently served or that any model is suitable. Suitability requires model-level environmental, regulatory, and performance data.

## Engineering capabilities

The supplied materials describe:

- AC/DC/EC motor-platform capability.
- CFD and aerodynamic design.
- Product R&D and application engineering.
- Customized ventilation solutions and OEM/ODM support.
- Engineering support through prototype and production introduction.

Status: `PROVISIONAL` organizational capability reported in the supplied company deck PDF pp. 4 and 7. Do not promise a specific custom deliverable, simulation accuracy, tooling ownership, IP term, sample time, or production capacity without owner and commercial confirmation.

## Production and test evidence

Reported production/test resources include automated welding, motor/rotor/stator production lines, dynamic balancing, runout checks, aging tests, wind-tunnel airflow testing, a noise room, salt-mist testing, temperature/humidity testing, and outdoor lifetime testing.

The reported quality-introduction flow is:

`Project initiation → Prototype → Pre-production sample → Pilot run`

Status: `PROVISIONAL` facilities/process claims shown in the supplied company deck PDF pp. 5–11. Do not turn a pictured capability into a model-specific test certificate or imply every product receives every test. Ask for the actual quality plan, sampling method, equipment calibration, standard, and report when acceptance depends on it.

## Terminology rules

- Airflow: use volume per time, commonly `m³/h` or `CFM`; record conversion rather than silently replacing source units.
- Pressure: distinguish static, total, and dynamic pressure; do not abbreviate a generic source value into a more specific type.
- PQ curve: pair airflow with pressure at declared speed, density, voltage, setup, and test standard where known.
- Noise: require metric (`dB(A)`, sound power, or sound pressure), distance, environment, operating point, and baseline.
- Efficiency and power: distinguish motor, fan, system, and input efficiency; distinguish electrical input, shaft output, and nominal power.
- Control: store signal/interface, supply, range, default behavior, feedback, and failure behavior separately.

## Source terminology corrections

Normalize these source residues in new content and structured data:

| Source text | Normalized text | Evidence/status |
|---|---|---|
| `Cenrifugal` | `Centrifugal` | `DEPRECATED` typo; company deck PDF p. 2 |
| `Backard Curved` | `Backward Curved` | `DEPRECATED` typo; company deck PDF p. 13 |
| `Hearing Pump` | `Heat Pump` | `DEPRECATED` typo; company deck PDF p. 12 |
| `Heating Convactors` | `Heating Convectors` | `DEPRECATED` typo; company deck PDF p. 12 |
| `Inet Duct Diameter` | `Inlet Duct Diameter` | `DEPRECATED` typo; company deck PDF p. 17 |
| `motos` | `motors` | `DEPRECATED` typo; company deck PDF p. 18 |
| `Isulation Class` | `Insulation Class` | `DEPRECATED` typo; company deck PDF p. 17 |

`BLEC Motor` on company deck PDF p. 17 is `CONFLICTED`: its intended technology is unknown. Do not silently rewrite it as BLDC, EC, or another motor type; require Product Master or engineering confirmation.
