mod app;
mod utils;

use std::collections::HashMap;
use std::io::BufReader;

use app::{bootstrap, App};
use ash::vk::PresentModeKHR;

use engine::{Backbuffer, Camera, DeferredRenderingPipeline, MaterialDescription, MaterialDomain, MaterialInstance, MaterialInstanceDescription, Mesh, MeshCreateInfo, MeshPrimitiveCreateInfo, RenderingPipeline, Scene, ScenePrimitive, Texture, TextureInput};
use nalgebra::*;
use resource_map::ResourceMap;
use winit::{event::ElementState, event_loop::EventLoop};
#[repr(C)]
#[derive(Clone, Copy)]
struct VertexData {
    pub position: Vector2<f32>,
    pub color: Vector3<f32>,
    pub uv: Vector2<f32>,
}
const SPEED: f32 = 0.1;
const ROTATION_SPEED: f32 = 3.0;
const MIN_DELTA: f32 = 1.0;
pub struct PlanesApp {
    resource_map: ResourceMap,
    camera: Camera,
    forward_movement: f32,
    rotation_movement: f32,
    rot_x: f32,
    rot_z: f32,
    dist: f32,
    movement: Vector3<f32>,
    scene_renderer: DeferredRenderingPipeline,
    scene: Scene,
}

impl App for PlanesApp {
    fn window_name(&self, _app_state: &engine::AppState) -> String {
        "planes".to_owned()
    }

    fn create(app_state: &engine::AppState, _: &EventLoop<()>) -> anyhow::Result<Self>
    where
        Self: Sized,
    {
        let mut resource_map = ResourceMap::new();

        let camera = Camera {
            location: point![2.0, 2.0, 2.0],
            forward: vector![0.0, -1.0, -1.0].normalize(),
            ..Default::default()
        };

        let forward_movement = 0.0;
        let rotation_movement = 0.0;

        let rot_x = 45.0;
        let rot_z = 55.0;
        let dist = 5.0;

        let movement: Vector3<f32> = vector![0.0, 0.0, 0.0];
        let cpu_image = image::load(
            BufReader::new(std::fs::File::open("images/texture.jpg")?),
            image::ImageFormat::Jpeg,
        )?;
        let cpu_image = cpu_image.into_rgba8();

        let vertex_module =
            utils::read_file_to_vk_module(&app_state.gpu, "./shaders/vertex_deferred.spirv")?;
        let fragment_module =
            utils::read_file_to_vk_module(&app_state.gpu, "./shaders/fragment_deferred.spirv")?;

        let mesh_data = MeshCreateInfo {
            label: Some("Quad mesh"),
            primitives: &[MeshPrimitiveCreateInfo {
                indices: vec![0, 1, 2, 2, 3, 0],
                positions: vec![
                    vector![-0.5, -0.5, 0.0],
                    vector![0.5, -0.5, 0.0],
                    vector![0.5, 0.5, 0.0],
                    vector![-0.5, 0.5, 0.0],
                ],
                colors: vec![
                    vector![1.0, 0.0, 0.0],
                    vector![0.0, 1.0, 0.0],
                    vector![0.0, 0.0, 1.0],
                    vector![1.0, 1.0, 1.0],
                ],
                normals: vec![
                    vector![0.0, 1.0, 0.0],
                    vector![0.0, 1.0, 0.0],
                    vector![0.0, 1.0, 0.0],
                    vector![0.0, 1.0, 0.0],
                ],
                tangents: vec![
                    vector![0.0, 0.0, 1.0],
                    vector![0.0, 0.0, 1.0],
                    vector![0.0, 0.0, 1.0],
                    vector![0.0, 0.0, 1.0],
                ],
                uvs: vec![
                    vector![1.0, 0.0],
                    vector![0.0, 0.0],
                    vector![0.0, 1.0],
                    vector![1.0, 1.0],
                ],
            }],
        };

        let mesh = Mesh::new(&app_state.gpu, &mesh_data)?;
        let mesh = resource_map.add(mesh);

        let texture = Texture::new_with_data(
            &app_state.gpu,
            &mut resource_map,
            cpu_image.width(),
            cpu_image.height(),
            &cpu_image,
            Some("Quad texture david"),
        )?;
        let texture = resource_map.add(texture);

        let screen_quad_module =
            utils::read_file_to_vk_module(&app_state.gpu, "./shaders/screen_quad.spirv")?;
        let gbuffer_combine_module =
            utils::read_file_to_vk_module(&app_state.gpu, "./shaders/gbuffer_combine.spirv")?;
        let texture_copy_module =
            utils::read_file_to_vk_module(&app_state.gpu, "./shaders/texture_copy.spirv")?;
        let tonemap_module =
            utils::read_file_to_vk_module(&app_state.gpu, "./shaders/tonemap.spirv")?;

        let mut scene_renderer = DeferredRenderingPipeline::new(
            &app_state.gpu,
            screen_quad_module,
            gbuffer_combine_module,
            texture_copy_module,
            tonemap_module,
        )?;

        let master = scene_renderer.create_material(
            &app_state.gpu,
            MaterialDescription {
                name: "Simple",
                domain: MaterialDomain::Surface,
                fragment_module: &fragment_module,
                vertex_module: &vertex_module,
                texture_inputs: &[TextureInput {
                    name: "texSampler".to_owned(),
                    format: gpu::ImageFormat::Rgba8,
                }],
                material_parameters: Default::default(),
            },
        )?;

        let mut texture_inputs = HashMap::new();
        texture_inputs.insert("texSampler".to_owned(), texture);
        let material = resource_map.add(master);
        let mat_instance = MaterialInstance::create_instance(
            &app_state.gpu,
            material,
            &resource_map,
            &MaterialInstanceDescription {
                name: "simple inst",
                texture_inputs,
            },
        )?;
        let mat_instance = resource_map.add(mat_instance);

        engine::app_state_mut()
            .gpu
            .swapchain_mut()
            .select_present_mode(PresentModeKHR::MAILBOX)?;

        let mut scene = Scene::new();

        scene.add(ScenePrimitive {
            mesh: mesh.clone(),
            materials: vec![mat_instance.clone()],
            transform: Matrix4::identity(),
        });
        scene.add(ScenePrimitive {
            mesh: mesh.clone(),
            materials: vec![mat_instance.clone()],
            transform: Matrix4::new_translation(&vector![0.0, 0.0, 1.0]),
        });
        scene.add(ScenePrimitive {
            mesh,
            materials: vec![mat_instance],
            transform: Matrix4::new_translation(&vector![0.0, 0.0, -1.0]),
        });
        Ok(Self {
            resource_map,
            camera,
            forward_movement,
            rotation_movement,
            rot_x,
            rot_z,
            dist,
            movement,
            scene_renderer,
            scene,
        })
    }

    fn input(
        &mut self,
        _app_state: &engine::AppState,
        event: winit::event::DeviceEvent,
    ) -> anyhow::Result<()> {
        match event {
            winit::event::DeviceEvent::Button { button, state } => {
                let mul = if state == ElementState::Pressed {
                    1.0
                } else {
                    0.0
                };
                if button == 3 {
                    self.rotation_movement = mul;
                } else if button == 1 {
                    self.forward_movement = mul;
                }
            }

            winit::event::DeviceEvent::MouseMotion { delta } => {
                self.movement.x = (delta.0.abs() as f32 - MIN_DELTA).max(0.0)
                    * delta.0.signum() as f32
                    * ROTATION_SPEED;
                self.movement.y = (delta.1.abs() as f32 - MIN_DELTA).max(0.0)
                    * delta.1.signum() as f32
                    * ROTATION_SPEED;
            }
            _ => {}
        };
        Ok(())
    }

    fn draw(&mut self, app_state: &mut engine::AppState) -> anyhow::Result<()> {
        let swapchain_format = app_state.gpu.swapchain().present_format();
        let swapchain_extents = app_state.gpu.swapchain().extents();
        let (swapchain_image, swapchain_image_view) =
            app_state.gpu.swapchain_mut().acquire_next_image()?;
        self.scene_renderer
            .render(
                &self.camera,
                &self.scene,
                Backbuffer {
                    size: swapchain_extents,
                    format: swapchain_format,
                    image: swapchain_image,
                    image_view: swapchain_image_view,
                },
                &self.resource_map,
            )
            .unwrap();

        Ok(())
    }

    fn update(&mut self, _app_state: &mut engine::AppState) -> anyhow::Result<()> {
        if self.rotation_movement > 0.0 {
            self.rot_z += self.movement.y;
            self.rot_z = self.rot_z.clamp(-89.0, 89.0);
            self.rot_x += self.movement.x;
        } else {
            self.dist += self.movement.y * self.forward_movement * SPEED;
        }

        let new_forward = Rotation::<f32, 3>::from_axis_angle(
            &Unit::new_normalize(vector![0.0, 0.0, 1.0]),
            self.rot_x.to_radians(),
        ) * Rotation::<f32, 3>::from_axis_angle(
            &Unit::new_normalize(vector![0.0, 1.0, 0.0]),
            -self.rot_z.to_radians(),
        );
        let new_forward = new_forward.to_homogeneous();
        let new_forward = new_forward.column(0);

        let direction = vector![new_forward[0], new_forward[1], new_forward[2]];
        let new_position = direction * self.dist;
        let new_position = point![new_position.x, new_position.y, new_position.z];
        self.camera.location = new_position;

        let direction = vector![new_forward[0], new_forward[1], new_forward[2]];
        self.camera.forward = -direction;
        Ok(())
    }
}

fn main() -> anyhow::Result<()> {
    bootstrap::<PlanesApp>()
}
