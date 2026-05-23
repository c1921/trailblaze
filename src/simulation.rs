use bevy::prelude::*;

#[derive(Resource, Debug, Clone)]
pub struct SimulationClock {
    pub paused: bool,
    pub speed: f32,
    pub elapsed: f32,
}

impl Default for SimulationClock {
    fn default() -> Self {
        Self {
            paused: false,
            speed: 1.0,
            elapsed: 0.0,
        }
    }
}

impl SimulationClock {
    pub fn scaled_delta(&self, time: &Time) -> f32 {
        if self.paused {
            0.0
        } else {
            time.delta_secs() * self.speed
        }
    }

    pub fn label(&self) -> String {
        if self.paused {
            "Paused".to_string()
        } else {
            format!("{:.0}x", self.speed)
        }
    }
}

pub fn control_time(
    keyboard: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
    mut clock: ResMut<SimulationClock>,
) {
    if keyboard.just_pressed(KeyCode::Space) {
        clock.paused = !clock.paused;
    }

    clock.elapsed += clock.scaled_delta(&time);
}
