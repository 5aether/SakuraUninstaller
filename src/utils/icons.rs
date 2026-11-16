use slint::{Image, Rgba8Pixel, SharedPixelBuffer};
use std::{
    cell::RefCell,
    collections::HashMap,
    path::{Path, PathBuf},
};

thread_local! {
    static CACHE: RefCell<HashMap<PathBuf, Image>> = RefCell::new(HashMap::new());
}

/// Returns the icon of `path` as an image, or an empty image when unavailable.
///
/// Image files (MSIX package logos, PNG/JPG `DisplayIcon` entries) are decoded as they
/// are, everything else goes through the shell icon of the file.
pub(crate) fn image(path: Option<&Path>) -> Image {
    let Some(path) = path else {
        return Image::default();
    };
    CACHE.with(|cache| {
        if let Some(image) = cache.borrow().get(path) {
            return image.clone();
        }
        let loaded = if is_asset(path) {
            Image::load_from_path(path).unwrap_or_default()
        } else {
            shell(path)
        };
        // Placeholder logos (1x1 files, fully transparent bitmaps) keep the initial.
        let image = if loaded.size().width < MIN_SIZE || loaded.size().height < MIN_SIZE {
            Image::default()
        } else {
            trimmed(loaded).unwrap_or_default()
        };
        cache.borrow_mut().insert(path.to_path_buf(), image.clone());
        image
    })
}

/// Crops the transparent border of a logo, so icons of different assets all fill the
/// same space in the avatar instead of looking bigger or smaller.
fn trimmed(image: Image) -> Option<Image> {
    let buffer = image.to_rgba8()?;
    let width = buffer.width() as usize;
    let height = buffer.height() as usize;
    let pixels = buffer.as_slice();
    let (mut left, mut top, mut right, mut bottom) = (width, height, 0usize, 0usize);
    for y in 0..height {
        for x in 0..width {
            if pixels[y * width + x].a == 0 {
                continue;
            }
            left = left.min(x);
            right = right.max(x + 1);
            top = top.min(y);
            bottom = bottom.max(y + 1);
        }
    }
    if left >= right || top >= bottom {
        return None;
    }
    if left == 0 && top == 0 && right == width && bottom == height {
        return Some(image);
    }
    let crop_width = right - left;
    let crop_height = bottom - top;
    if crop_width < MIN_SIZE as usize || crop_height < MIN_SIZE as usize {
        return None;
    }
    let mut cropped = SharedPixelBuffer::<Rgba8Pixel>::new(crop_width as u32, crop_height as u32);
    let target = cropped.make_mut_slice();
    for y in 0..crop_height {
        let start = (top + y) * width + left;
        target[y * crop_width..(y + 1) * crop_width]
            .copy_from_slice(&pixels[start..start + crop_width]);
    }
    Some(Image::from_rgba8(cropped))
}

fn is_asset(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| {
            matches!(
                extension.to_ascii_lowercase().as_str(),
                "png" | "jpg" | "jpeg" | "bmp"
            )
        })
        .unwrap_or(false)
}

/// Largest icon the file carries; smaller sizes are tried when the big one is blank
/// (some 256px PNG-compressed icons do not rasterize).
fn shell(path: &Path) -> Image {
    for size in [SIZE, 32] {
        let Some(pixels) = raster(path, size) else {
            continue;
        };
        if pixels.chunks(4).all(|pixel| pixel[3] == 0) {
            continue;
        }
        let mut buffer = SharedPixelBuffer::<Rgba8Pixel>::new(size as u32, size as u32);
        for (index, pixel) in buffer.make_mut_slice().iter_mut().enumerate() {
            *pixel = Rgba8Pixel {
                r: pixels[index * 4],
                g: pixels[index * 4 + 1],
                b: pixels[index * 4 + 2],
                a: pixels[index * 4 + 3],
            };
        }
        return Image::from_rgba8(buffer);
    }
    Image::default()
}

const SIZE: i32 = 64;
const MIN_SIZE: u32 = 8;

#[cfg(windows)]
fn raster(path: &Path, size: i32) -> Option<Vec<u8>> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::{
        Graphics::Gdi::{
            BI_RGB, BITMAPINFO, BITMAPINFOHEADER, CreateCompatibleDC, CreateDIBSection,
            DIB_RGB_COLORS, DeleteDC, DeleteObject, GetDC, ReleaseDC, SelectObject,
        },
        UI::WindowsAndMessaging::{DI_NORMAL, DestroyIcon, DrawIconEx},
    };

    let wide: Vec<u16> = path.as_os_str().encode_wide().chain([0]).collect();
    let icon = icon_handle(&wide, size);
    if icon.is_null() {
        return None;
    }

    let screen = unsafe { GetDC(std::ptr::null_mut()) };
    let dc = unsafe { CreateCompatibleDC(screen) };
    let mut header = unsafe { std::mem::zeroed::<BITMAPINFO>() };
    header.bmiHeader = BITMAPINFOHEADER {
        biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
        biWidth: size,
        biHeight: -size,
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
                icon,
                size,
                size,
                0,
                std::ptr::null_mut(),
                DI_NORMAL,
            );
        }
        let raw =
            unsafe { std::slice::from_raw_parts(bits as *const u8, (size * size * 4) as usize) };
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
        DestroyIcon(icon);
    }
    pixels
}

/// Biggest icon the file itself carries (up to [`SIZE`]), falling back to the shell icon.
#[cfg(windows)]
fn icon_handle(wide: &[u16], size: i32) -> windows_sys::Win32::UI::WindowsAndMessaging::HICON {
    use windows_sys::Win32::UI::{
        Shell::{SHFILEINFOW, SHGFI_ICON, SHGFI_LARGEICON, SHGetFileInfoW},
        WindowsAndMessaging::HICON,
    };

    let mut icon: HICON = std::ptr::null_mut();
    let count = unsafe {
        windows_sys::Win32::UI::WindowsAndMessaging::PrivateExtractIconsW(
            wide.as_ptr(),
            0,
            size,
            size,
            &mut icon,
            std::ptr::null_mut(),
            1,
            0,
        )
    };
    if count > 0 && !icon.is_null() {
        return icon;
    }
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
    if found == 0 {
        std::ptr::null_mut()
    } else {
        info.hIcon
    }
}

#[cfg(not(windows))]
fn raster(_path: &Path, _size: i32) -> Option<Vec<u8>> {
    None
}
