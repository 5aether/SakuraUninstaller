use slint::{Image, Rgba8Pixel, SharedPixelBuffer};
use std::{
    cell::RefCell,
    collections::HashMap,
    path::{Path, PathBuf},
};

thread_local! {
    static CACHE: RefCell<HashMap<PathBuf, Image>> = RefCell::new(HashMap::new());
}

/// Returns the shell icon of `path` as an image, or an empty image when unavailable.
pub(crate) fn image(path: Option<&Path>) -> Image {
    let Some(path) = path else {
        return Image::default();
    };
    CACHE.with(|cache| {
        if let Some(image) = cache.borrow().get(path) {
            return image.clone();
        }
        let image = raster(path).map_or_else(Image::default, |pixels| {
            let mut buffer = SharedPixelBuffer::<Rgba8Pixel>::new(SIZE as u32, SIZE as u32);
            for (index, pixel) in buffer.make_mut_slice().iter_mut().enumerate() {
                *pixel = Rgba8Pixel {
                    r: pixels[index * 4],
                    g: pixels[index * 4 + 1],
                    b: pixels[index * 4 + 2],
                    a: pixels[index * 4 + 3],
                };
            }
            Image::from_rgba8(buffer)
        });
        cache.borrow_mut().insert(path.to_path_buf(), image.clone());
        image
    })
}

const SIZE: i32 = 32;

#[cfg(windows)]
fn raster(path: &Path) -> Option<Vec<u8>> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::{
        Graphics::Gdi::{
            BI_RGB, BITMAPINFO, BITMAPINFOHEADER, CreateCompatibleDC, CreateDIBSection,
            DIB_RGB_COLORS, DeleteDC, DeleteObject, GetDC, ReleaseDC, SelectObject,
        },
        UI::{
            Shell::{SHFILEINFOW, SHGFI_ICON, SHGFI_LARGEICON, SHGetFileInfoW},
            WindowsAndMessaging::{DI_NORMAL, DestroyIcon, DrawIconEx},
        },
    };

    let wide: Vec<u16> = path.as_os_str().encode_wide().chain([0]).collect();
    let mut info = unsafe { std::mem::zeroed::<SHFILEINFOW>() };
    let found = unsafe {
        SHGetFileInfoW(
            wide.as_ptr(),
            0,
            &mut info,
            std::mem::size_of::<SHFILEINFOW>() as u32,
            SHGFI_ICON | SHGFI_LARGEICON,
        )
    };
    if found == 0 || info.hIcon.is_null() {
        return None;
    }

    let screen = unsafe { GetDC(std::ptr::null_mut()) };
    let dc = unsafe { CreateCompatibleDC(screen) };
    let mut header = unsafe { std::mem::zeroed::<BITMAPINFO>() };
    header.bmiHeader = BITMAPINFOHEADER {
        biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
        biWidth: SIZE,
        biHeight: -SIZE,
        biPlanes: 1,
        biBitCount: 32,
        biCompression: BI_RGB,
        ..unsafe { std::mem::zeroed() }
    };
    let mut bits: *mut std::ffi::c_void = std::ptr::null_mut();
    let bitmap = unsafe {
        CreateDIBSection(
            dc,
            &header,
            DIB_RGB_COLORS,
            &mut bits,
            std::ptr::null_mut(),
            0,
        )
    };

    let mut pixels = None;
    if !screen.is_null() && !dc.is_null() && !bitmap.is_null() && !bits.is_null() {
        unsafe {
            SelectObject(dc, bitmap);
            DrawIconEx(
                dc,
                0,
                0,
                info.hIcon,
                SIZE,
                SIZE,
                0,
                std::ptr::null_mut(),
                DI_NORMAL,
            );
        }
        let raw =
            unsafe { std::slice::from_raw_parts(bits as *const u8, (SIZE * SIZE * 4) as usize) };
        let mut rgba = vec![0u8; raw.len()];
        for index in 0..rgba.len() / 4 {
            let alpha = raw[index * 4 + 3];
            rgba[index * 4 + 3] = alpha;
            if alpha != 0 {
                let unpremultiply = |channel: u8| {
                    ((u32::from(channel) * 255 + u32::from(alpha) / 2) / u32::from(alpha)).min(255)
                        as u8
                };
                rgba[index * 4] = unpremultiply(raw[index * 4 + 2]);
                rgba[index * 4 + 1] = unpremultiply(raw[index * 4 + 1]);
                rgba[index * 4 + 2] = unpremultiply(raw[index * 4]);
            }
        }
        pixels = Some(rgba);
    }

    unsafe {
        if !bitmap.is_null() {
            DeleteObject(bitmap);
        }
        if !dc.is_null() {
            DeleteDC(dc);
        }
        if !screen.is_null() {
            ReleaseDC(std::ptr::null_mut(), screen);
        }
        DestroyIcon(info.hIcon);
    }
    pixels
}

#[cfg(not(windows))]
fn raster(_path: &Path) -> Option<Vec<u8>> {
    None
}
