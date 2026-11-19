//! Unsichtbarer OpenGL-Kontext für die HD-Filter.
//!
//! GPUI zeichnet mit DirectX und erlaubt keine eigenen Shader. Die Filter-Shader laufen deshalb in einem
//! eigenen, unsichtbaren OpenGL-Fenster: Sie rendern in ein Bild in Fenstergröße, das danach als
//! fertiges Bild an GPUI geht (1:1 in Bildschirmpixeln, also genauso scharf wie vorher).

use glutin::config::ConfigTemplateBuilder;
use glutin::context::{ContextApi, ContextAttributesBuilder, GlProfile, NotCurrentGlContext, PossiblyCurrentContext, Version};
use glutin::display::{Display, DisplayApiPreference, GlDisplay};
use glutin::surface::{GlSurface, Surface, SurfaceAttributesBuilder, WindowSurface};
use raw_window_handle::{RawDisplayHandle, RawWindowHandle, Win32WindowHandle, WindowsDisplayHandle};
use std::num::{NonZeroIsize, NonZeroU32};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::WindowsAndMessaging::{CreateWindowExW, WS_POPUP};

pub struct OffGl {
    pub gl: glow::Context,
    ctx: PossiblyCurrentContext,
    surf: Surface<WindowSurface>,
}

impl OffGl {
    pub fn new() -> Result<OffGl, String> {
        unsafe {
            let cls: Vec<u16> = "STATIC\0".encode_utf16().collect();
            let hinst = GetModuleHandleW(std::ptr::null());
            let hwnd = CreateWindowExW(0, cls.as_ptr(), std::ptr::null(), WS_POPUP, 0, 0, 16, 16, std::ptr::null_mut(), std::ptr::null_mut(), hinst, std::ptr::null());
            let hwnd = NonZeroIsize::new(hwnd as isize).ok_or("Hilfsfenster für OpenGL")?;
            let mut wh = Win32WindowHandle::new(hwnd);
            wh.hinstance = NonZeroIsize::new(hinst as isize);
            let rwh = RawWindowHandle::Win32(wh);
            let disp = Display::new(RawDisplayHandle::Windows(WindowsDisplayHandle::new()), DisplayApiPreference::Wgl(Some(rwh))).map_err(|e| e.to_string())?;
            let cfg = disp.find_configs(ConfigTemplateBuilder::new().compatible_with_native_window(rwh).build()).map_err(|e| e.to_string())?.next().ok_or("keine OpenGL-Konfiguration")?;
            let ca = ContextAttributesBuilder::new().with_profile(GlProfile::Compatibility).with_context_api(ContextApi::OpenGl(Some(Version::new(3, 3)))).build(Some(rwh));
            let nc = disp.create_context(&cfg, &ca).map_err(|e| e.to_string())?;
            let one = NonZeroU32::new(16).unwrap();
            let surf = disp.create_window_surface(&cfg, &SurfaceAttributesBuilder::<WindowSurface>::new().build(rwh, one, one)).map_err(|e| e.to_string())?;
            let ctx = nc.make_current(&surf).map_err(|e| e.to_string())?;
            let gl = glow::Context::from_loader_function_cstr(|s| disp.get_proc_address(s));
            Ok(OffGl { gl, ctx, surf })
        }
    }

    /// Kontext für diesen Thread aktiv machen (falls ihn jemand anderes übernommen hat)
    pub fn make_current(&self) {
        use glutin::context::PossiblyCurrentGlContext;
        if !self.ctx.is_current() {
            let _ = self.ctx.make_current(&self.surf);
        }
        let _ = self.surf.is_single_buffered();
    }
}
