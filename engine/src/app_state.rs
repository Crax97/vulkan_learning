use ash::prelude::VkResult;
use gpu::{Gpu, Swapchain};

use crate::Time;

pub struct AppState {
    pub gpu: Gpu,
    pub swapchain: Swapchain,
    time: Time,
}
impl AppState {
    pub fn new(gpu: Gpu, swapchain: Swapchain) -> Self {
        Self {
            gpu,
            swapchain,
            time: Time::new(),
        }
    }

    pub fn begin_frame(&mut self) -> VkResult<()> {
        self.time.update();
        Ok(())
    }

    pub fn end_frame(&mut self) -> VkResult<()> {
        self.gpu.reset_state()?;
        Ok(())
    }

    pub fn time(&self) -> &Time {
        &self.time
    }
}
