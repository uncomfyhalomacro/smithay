pub use drm::{
    control::{connector, crtc, framebuffer, Device as ControlDevice, Mode, ResourceHandles, ResourceInfo},
    Device as BasicDevice,
};
pub use nix::libc::dev_t;

use std::error::Error;
use std::iter::IntoIterator;
use std::os::unix::io::AsRawFd;
use std::path::PathBuf;

use wayland_server::calloop::generic::{EventedFd, Generic};
use wayland_server::calloop::mio::Ready;
pub use wayland_server::calloop::InsertError;
use wayland_server::calloop::{LoopHandle, Source};

use super::graphics::SwapBuffersError;

#[cfg(feature = "backend_drm_egl")]
pub mod egl;
#[cfg(feature = "backend_drm_gbm")]
pub mod gbm;
#[cfg(feature = "backend_drm_legacy")]
pub mod legacy;

pub trait DeviceHandler {
    type Device: Device + ?Sized;
    fn vblank(&mut self, crtc: crtc::Handle);
    fn error(&mut self, error: <<<Self as DeviceHandler>::Device as Device>::Surface as Surface>::Error);
}

pub trait Device: AsRawFd + DevPath {
    type Surface: Surface;

    fn device_id(&self) -> dev_t;
    fn set_handler(&mut self, handler: impl DeviceHandler<Device = Self> + 'static);
    fn clear_handler(&mut self);
    fn create_surface(
        &mut self,
        ctrc: crtc::Handle,
    ) -> Result<Self::Surface, <Self::Surface as Surface>::Error>;
    fn process_events(&mut self);
    fn resource_info<T: ResourceInfo>(
        &self,
        handle: T::Handle,
    ) -> Result<T, <Self::Surface as Surface>::Error>;
    fn resource_handles(&self) -> Result<ResourceHandles, <Self::Surface as Surface>::Error>;
}

pub trait RawDevice: Device<Surface = <Self as RawDevice>::Surface> {
    type Surface: RawSurface;
}

pub trait Surface {
    type Connectors: IntoIterator<Item = connector::Handle>;
    type Error: Error + Send;

    fn crtc(&self) -> crtc::Handle;
    fn current_connectors(&self) -> Self::Connectors;
    fn pending_connectors(&self) -> Self::Connectors;
    fn add_connector(&self, connector: connector::Handle) -> Result<(), Self::Error>;
    fn remove_connector(&self, connector: connector::Handle) -> Result<(), Self::Error>;
    fn current_mode(&self) -> Option<Mode>;
    fn pending_mode(&self) -> Option<Mode>;
    fn use_mode(&self, mode: Option<Mode>) -> Result<(), Self::Error>;
}

pub trait RawSurface: Surface + ControlDevice + BasicDevice {
    fn commit_pending(&self) -> bool;
    fn commit(&self, framebuffer: framebuffer::Handle) -> Result<(), <Self as Surface>::Error>;
    fn page_flip(&self, framebuffer: framebuffer::Handle) -> Result<(), SwapBuffersError>;
}

/// Trait for types representing open devices
pub trait DevPath {
    /// Returns the path of the open device if possible
    fn dev_path(&self) -> Option<PathBuf>;
}

impl<A: AsRawFd> DevPath for A {
    fn dev_path(&self) -> Option<PathBuf> {
        use std::fs;

        fs::read_link(format!("/proc/self/fd/{:?}", self.as_raw_fd())).ok()
    }
}

/// Bind a `Device` to an `EventLoop`,
///
/// This will cause it to recieve events and feed them into an `DeviceHandler`
pub fn device_bind<D: Device + 'static, Data>(
    handle: &LoopHandle<Data>,
    device: D,
) -> ::std::result::Result<Source<Generic<EventedFd<D>>>, InsertError<Generic<EventedFd<D>>>>
where
    D: Device,
    Data: 'static,
{
    let mut source = Generic::from_fd_source(device);
    source.set_interest(Ready::readable());

    handle.insert_source(source, |evt, _| {
        evt.source.borrow_mut().0.process_events();
    })
}
