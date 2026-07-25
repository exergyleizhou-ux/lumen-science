//! DummyLab — simulated laboratory for EmbodiedExperimentSessionV0 (Track D).
//!
//! Provides read-only sensors and simulated actuators that route through the
//! SessionActor → Permission → artifact → replay path *without* touching real
//! hardware. This is the first step before connecting real devices (SSH, HPC,
//! BOS, wet-lab equipment).
//!
//! Design principle: the DummyLab must exercise the SAME permission and
//! evidence pipeline as a real lab. Only the I/O is fake.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

// ── Sensor ──────────────────────────────────────────────────────────

/// A simulated sensor that produces bounded random readings.
#[derive(Debug)]
pub struct DummySensor {
    pub id: String,
    pub kind: SensorKind,
    /// Minimum plausible value.
    pub min: f64,
    /// Maximum plausible value.
    pub max: f64,
    /// Drift per second (positive or negative).
    pub drift: f64,
    last_value: Arc<AtomicU64>,
    last_read: Instant,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SensorKind {
    Temperature,
    Pressure,
    Humidity,
    Ph,
    OpticalDensity,
    Mass,
    Voltage,
    Custom(String),
}

impl DummySensor {
    pub fn new(id: impl Into<String>, kind: SensorKind, min: f64, max: f64) -> Self {
        Self {
            id: id.into(),
            kind,
            min,
            max,
            drift: 0.0,
            last_value: Arc::new(AtomicU64::new((min + (max - min) / 2.0).to_bits())),
            last_read: Instant::now(),
        }
    }

    pub fn with_drift(mut self, drift: f64) -> Self {
        self.drift = drift;
        self
    }

    /// Read the current simulated value, applying drift since last read.
    pub fn read(&mut self) -> f64 {
        let elapsed = self.last_read.elapsed().as_secs_f64();
        let current = f64::from_bits(self.last_value.load(Ordering::Relaxed));
        let drifted = current + self.drift * elapsed;
        let clamped = drifted.clamp(self.min, self.max);
        self.last_value.store(clamped.to_bits(), Ordering::Relaxed);
        self.last_read = Instant::now();
        clamped
    }
}

// ── Actuator ────────────────────────────────────────────────────────

/// A simulated actuator that logs actions instead of controlling real hardware.
#[derive(Debug)]
pub struct DummyActuator {
    pub id: String,
    pub kind: ActuatorKind,
    /// Log of all actions performed (for replay verification).
    pub log: Vec<ActuatorAction>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ActuatorKind {
    Pump,
    Valve,
    Heater,
    Stirrer,
    Dispenser,
    Custom(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct ActuatorAction {
    pub actuator_id: String,
    pub action: String,
    pub target_value: f64,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

impl serde::Serialize for ActuatorAction {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut st = s.serialize_struct("ActuatorAction", 4)?;
        st.serialize_field("actuator_id", &self.actuator_id)?;
        st.serialize_field("action", &self.action)?;
        st.serialize_field("target_value", &self.target_value)?;
        st.serialize_field("timestamp", &self.timestamp.to_rfc3339())?;
        st.end()
    }
}

impl<'de> serde::Deserialize<'de> for ActuatorAction {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let v = serde_json::Value::deserialize(d)?;
        Ok(ActuatorAction {
            actuator_id: v["actuator_id"].as_str().unwrap_or("").to_string(),
            action: v["action"].as_str().unwrap_or("").to_string(),
            target_value: v["target_value"].as_f64().unwrap_or(0.0),
            timestamp: v["timestamp"].as_str()
                .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                .map(|dt| dt.with_timezone(&chrono::Utc))
                .unwrap_or_else(chrono::Utc::now),
        })
    }
}

impl DummyActuator {
    pub fn new(id: impl Into<String>, kind: ActuatorKind) -> Self {
        Self {
            id: id.into(),
            kind,
            log: Vec::new(),
        }
    }

    /// Simulate an actuation. Logs the action and returns it for evidence.
    pub fn actuate(&mut self, action: impl Into<String>, target_value: f64) -> ActuatorAction {
        let entry = ActuatorAction {
            actuator_id: self.id.clone(),
            action: action.into(),
            target_value,
            timestamp: chrono::Utc::now(),
        };
        self.log.push(entry.clone());
        entry
    }
}

// ── Lab ─────────────────────────────────────────────────────────────

/// A complete simulated lab with sensors, actuators, and an evidence log.
///
/// This is the `EmbodiedExperimentSessionV0` data model. All mutations go
/// through `apply_command` which returns an `LabEvidence` record suitable for
/// the SessionActor → artifact → replay pipeline.
#[derive(Debug)]
pub struct DummyLab {
    pub id: String,
    pub sensors: BTreeMap<String, DummySensor>,
    pub actuators: BTreeMap<String, DummyActuator>,
    pub evidence: Vec<LabEvidence>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct LabEvidence {
    pub lab_id: String,
    pub sequence: u64,
    pub sensor_readings: BTreeMap<String, f64>,
    pub actuator_actions: Vec<ActuatorAction>,
}

impl DummyLab {
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            sensors: BTreeMap::new(),
            actuators: BTreeMap::new(),
            evidence: Vec::new(),
        }
    }

    pub fn with_sensor(mut self, sensor: DummySensor) -> Self {
        self.sensors.insert(sensor.id.clone(), sensor);
        self
    }

    pub fn with_actuator(mut self, actuator: DummyActuator) -> Self {
        self.actuators.insert(actuator.id.clone(), actuator);
        self
    }

    /// Read all sensors and log the snapshot as evidence.
    pub fn snapshot(&mut self) -> LabEvidence {
        let readings: BTreeMap<String, f64> = self
            .sensors
            .iter_mut()
            .map(|(id, sensor)| (id.clone(), sensor.read()))
            .collect();
        let evidence = LabEvidence {
            lab_id: self.id.clone(),
            sequence: self.evidence.len() as u64,
            sensor_readings: readings,
            actuator_actions: Vec::new(),
        };
        self.evidence.push(evidence.clone());
        evidence
    }

    /// Actuate a device and generate evidence.
    pub fn run_action(
        &mut self,
        actuator_id: &str,
        action: &str,
        target: f64,
    ) -> Option<LabEvidence> {
        let actuator = self.actuators.get_mut(actuator_id)?;
        let action_entry = actuator.actuate(action, target);
        let readings: BTreeMap<String, f64> = self
            .sensors
            .iter_mut()
            .map(|(id, sensor)| (id.clone(), sensor.read()))
            .collect();
        let evidence = LabEvidence {
            lab_id: self.id.clone(),
            sequence: self.evidence.len() as u64,
            sensor_readings: readings,
            actuator_actions: vec![action_entry],
        };
        self.evidence.push(evidence.clone());
        Some(evidence)
    }
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dummy_sensor_reads_within_bounds() {
        let mut sensor = DummySensor::new("temp-1", SensorKind::Temperature, 20.0, 30.0);
        for _ in 0..100 {
            let value = sensor.read();
            assert!(value >= 20.0 && value <= 30.0);
        }
    }

    #[test]
    fn dummy_sensor_drifts_over_time() {
        let mut sensor = DummySensor::new("ph-1", SensorKind::Ph, 0.0, 14.0)
            .with_drift(0.1);
        let first = sensor.read();
        std::thread::sleep(std::time::Duration::from_secs(1));
        let second = sensor.read();
        assert!((second - first).abs() > 0.0, "value should drift");
    }

    #[test]
    fn dummy_lab_snapshot_produces_evidence() {
        let mut lab = DummyLab::new("test-lab")
            .with_sensor(DummySensor::new("temp", SensorKind::Temperature, 20.0, 25.0))
            .with_sensor(DummySensor::new("ph", SensorKind::Ph, 6.0, 8.0))
            .with_actuator(DummyActuator::new("pump-a", ActuatorKind::Pump));

        let snap = lab.snapshot();
        assert_eq!(snap.sequence, 0);
        assert_eq!(snap.sensor_readings.len(), 2);
        assert_eq!(lab.evidence.len(), 1);
    }

    #[test]
    fn dummy_lab_action_produces_evidence() {
        let mut lab = DummyLab::new("action-lab")
            .with_sensor(DummySensor::new("od", SensorKind::OpticalDensity, 0.0, 2.0))
            .with_actuator(DummyActuator::new("disp", ActuatorKind::Dispenser));

        let evidence = lab.run_action("disp", "dispense", 0.5).unwrap();
        assert_eq!(evidence.sequence, 0);
        assert_eq!(evidence.actuator_actions.len(), 1);
        assert_eq!(evidence.actuator_actions[0].action, "dispense");
        assert_eq!(lab.evidence.len(), 1);
    }

    #[test]
    fn dummy_lab_unknown_actuator_returns_none() {
        let mut lab = DummyLab::new("bad-lab");
        let result = lab.run_action("nonexistent", "open", 1.0);
        assert!(result.is_none());
    }
}
