use std::f32::consts::{FRAC_PI_4, FRAC_PI_2};

use bevy::{
    input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll},
    prelude::*,
};

const GRID_CELLS: u32 = 48;
const GRID_SPACING: f32 = 1.0;
const GRID_AXIS_Y: f32 = 0.02;

fn main() {
    App::new()
        .insert_resource(ClearColor(Color::srgb(0.76, 0.8, 0.86)))
        .add_plugins(DefaultPlugins)
        .add_systems(Startup, setup_scene)
        .add_systems(Update, (control_camera, draw_grid))
        .run();
}

#[derive(Component)]
struct OrbitCamera {
    focus: Vec3,
    radius: f32,
    yaw: f32,
    pitch: f32,
    rotate_sensitivity: f32,
    pan_sensitivity: f32,
    zoom_sensitivity: f32,
    min_radius: f32,
    max_radius: f32,
    min_pitch: f32,
    max_pitch: f32,
}

impl Default for OrbitCamera {
    fn default() -> Self {
        Self {
            focus: Vec3::new(0.0, 0.5, 0.0),
            radius: 8.0,
            yaw: -FRAC_PI_4,
            pitch: FRAC_PI_4,
            rotate_sensitivity: 0.006,
            pan_sensitivity: 0.0025,
            zoom_sensitivity: 0.14,
            min_radius: 2.0,
            max_radius: 35.0,
            min_pitch: 0.18,
            max_pitch: FRAC_PI_2 - 0.08,
        }
    }
}

fn setup_scene(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    commands.spawn((
        Mesh3d(meshes.add(Plane3d::default().mesh().size(50.0, 50.0))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.62, 0.65, 0.69),
            perceptual_roughness: 0.8,
            ..default()
        })),
    ));

    commands.spawn((
        Mesh3d(meshes.add(Cuboid::from_length(1.0))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.18, 0.46, 0.85),
            perceptual_roughness: 0.55,
            ..default()
        })),
        Transform::from_xyz(0.0, 0.5, 0.0),
    ));

    commands.spawn((
        DirectionalLight {
            illuminance: 12_000.0,
            shadows_enabled: true,
            ..default()
        },
        Transform::from_xyz(5.0, 8.0, 5.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));

    let orbit_camera = OrbitCamera::default();
    let mut transform = Transform::default();
    sync_camera_transform(&orbit_camera, &mut transform);

    commands.spawn((Camera3d::default(), transform, orbit_camera));
}

fn draw_grid(mut gizmos: Gizmos) {
    gizmos.grid(
        Isometry3d::new(Vec3::Y * GRID_AXIS_Y, Quat::from_rotation_x(FRAC_PI_2)),
        UVec2::splat(GRID_CELLS),
        Vec2::splat(GRID_SPACING),
        LinearRgba::gray(0.45),
    );

    let half_extent = GRID_CELLS as f32 * GRID_SPACING * 0.5;
    gizmos.line(
        Vec3::new(-half_extent, GRID_AXIS_Y, 0.0),
        Vec3::new(half_extent, GRID_AXIS_Y, 0.0),
        LinearRgba::rgb(0.9, 0.16, 0.14),
    );
    gizmos.line(
        Vec3::new(0.0, GRID_AXIS_Y, -half_extent),
        Vec3::new(0.0, GRID_AXIS_Y, half_extent),
        LinearRgba::rgb(0.16, 0.35, 0.95),
    );
}

fn control_camera(
    mouse_buttons: Res<ButtonInput<MouseButton>>,
    mouse_motion: Res<AccumulatedMouseMotion>,
    mouse_scroll: Res<AccumulatedMouseScroll>,
    mut camera_query: Query<(&mut OrbitCamera, &mut Transform)>,
) {
    let Ok((mut camera, mut transform)) = camera_query.single_mut() else {
        return;
    };

    let drag_delta = mouse_motion.delta;
    if mouse_buttons.pressed(MouseButton::Right) && drag_delta != Vec2::ZERO {
        camera.yaw -= drag_delta.x * camera.rotate_sensitivity;
        camera.pitch = (camera.pitch + drag_delta.y * camera.rotate_sensitivity)
            .clamp(camera.min_pitch, camera.max_pitch);
    }

    if mouse_buttons.pressed(MouseButton::Middle) && drag_delta != Vec2::ZERO {
        let right = transform.rotation * Vec3::X;
        let mut forward = transform.rotation * -Vec3::Z;
        forward.y = 0.0;
        forward = forward.normalize_or_zero();

        let pan_scale = camera.radius * camera.pan_sensitivity;
        camera.focus += (-right * drag_delta.x + forward * drag_delta.y) * pan_scale;
    }

    let scroll_delta = mouse_scroll.delta.y;
    if scroll_delta.abs() > f32::EPSILON {
        let zoom_factor = (-scroll_delta * camera.zoom_sensitivity).exp();
        camera.radius = (camera.radius * zoom_factor).clamp(camera.min_radius, camera.max_radius);
    }

    sync_camera_transform(&camera, &mut transform);
}

fn sync_camera_transform(camera: &OrbitCamera, transform: &mut Transform) {
    let horizontal_radius = camera.radius * camera.pitch.cos();
    let offset = Vec3::new(
        horizontal_radius * camera.yaw.sin(),
        camera.radius * camera.pitch.sin(),
        horizontal_radius * camera.yaw.cos(),
    );

    transform.translation = camera.focus + offset;
    transform.look_at(camera.focus, Vec3::Y);
}
