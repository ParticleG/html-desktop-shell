use gtk4::{gdk_pixbuf, gio, prelude::*};

const MAX_DIMENSION: i32 = 512;
const MAX_ICON_BYTES: usize = 1024 * 1024;

#[derive(Clone, Debug, serde::Serialize)]
pub(crate) struct Icon {
    pub width: u32,
    pub height: u32,
    pub rgba: String,
}

pub(crate) fn theme_icon(name: &str, size: i32) -> Option<Icon> {
    if name.is_empty() || name.contains(['/', '\\', '\0']) || !gtk4::is_initialized_main_thread() {
        return None;
    }
    let display = gtk4::gdk::Display::default()?;
    let theme = gtk4::IconTheme::for_display(&display);
    if !theme.has_icon(name) {
        return None;
    }
    let paintable = theme.lookup_icon(
        name,
        &[],
        size.clamp(1, MAX_DIMENSION),
        1,
        gtk4::TextDirection::None,
        gtk4::IconLookupFlags::empty(),
    );
    file_icon(&paintable.file()?, size)
}

pub(crate) fn from_gicon(icon: &impl IsA<gio::Icon>, size: i32) -> Option<Icon> {
    if !gtk4::is_initialized_main_thread() {
        return None;
    }
    let display = gtk4::gdk::Display::default()?;
    let theme = gtk4::IconTheme::for_display(&display);
    let paintable = theme.lookup_by_gicon(
        icon,
        size.clamp(1, MAX_DIMENSION),
        1,
        gtk4::TextDirection::None,
        gtk4::IconLookupFlags::empty(),
    );
    file_icon(&paintable.file()?, size)
}

fn file_icon(file: &gio::File, size: i32) -> Option<Icon> {
    // Only native theme/installed application resolution supplies these files.
    let path = file.path()?;
    let size = size.clamp(1, MAX_DIMENSION);
    let pixbuf = gdk_pixbuf::Pixbuf::from_file_at_scale(path, size, size, true).ok()?;
    from_pixbuf(&pixbuf)
}

pub(crate) fn from_argb(width: i32, height: i32, bytes: &[u8]) -> Option<Icon> {
    let pixels = pixel_count(width, height)?;
    if bytes.len() != pixels.checked_mul(4)? {
        return None;
    }
    let mut rgba = Vec::with_capacity(bytes.len());
    for pixel in bytes.chunks_exact(4) {
        rgba.extend_from_slice(&[pixel[1], pixel[2], pixel[3], pixel[0]]);
    }
    Some(encode(width, height, &rgba))
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn from_rgb(
    width: i32,
    height: i32,
    rowstride: i32,
    has_alpha: bool,
    bits: i32,
    channels: i32,
    bytes: &[u8],
) -> Option<Icon> {
    let pixels = pixel_count(width, height)?;
    if bits != 8 || channels != if has_alpha { 4 } else { 3 } || bytes.len() > MAX_ICON_BYTES {
        return None;
    }
    let width_usize = usize::try_from(width).ok()?;
    let height_usize = usize::try_from(height).ok()?;
    let channels_usize = usize::try_from(channels).ok()?;
    let row_bytes = width_usize.checked_mul(channels_usize)?;
    let rowstride = usize::try_from(rowstride).ok()?;
    if rowstride < row_bytes
        || bytes.len()
            < rowstride
                .checked_mul(height_usize - 1)?
                .checked_add(row_bytes)?
    {
        return None;
    }
    let mut rgba = Vec::with_capacity(pixels.checked_mul(4)?);
    for row in 0..height_usize {
        let offset = row.checked_mul(rowstride)?;
        for pixel in bytes[offset..offset + row_bytes].chunks_exact(channels_usize) {
            rgba.extend_from_slice(&pixel[..3]);
            rgba.push(if has_alpha { pixel[3] } else { 255 });
        }
    }
    Some(encode(width, height, &rgba))
}

pub(crate) fn from_png(bytes: &[u8]) -> Option<Icon> {
    if bytes.len() < 24 || bytes.len() > MAX_ICON_BYTES || &bytes[..8] != b"\x89PNG\r\n\x1a\n" {
        return None;
    }
    if &bytes[12..16] != b"IHDR" {
        return None;
    }
    let width = i32::try_from(u32::from_be_bytes(bytes[16..20].try_into().ok()?)).ok()?;
    let height = i32::try_from(u32::from_be_bytes(bytes[20..24].try_into().ok()?)).ok()?;
    pixel_count(width, height)?;
    let stream = gio::MemoryInputStream::from_bytes(&glib::Bytes::from(bytes));
    let pixbuf = gdk_pixbuf::Pixbuf::from_stream(&stream, gio::Cancellable::NONE).ok()?;
    from_pixbuf(&pixbuf)
}

fn from_pixbuf(pixbuf: &gdk_pixbuf::Pixbuf) -> Option<Icon> {
    let bytes = pixbuf.read_pixel_bytes();
    from_rgb(
        pixbuf.width(),
        pixbuf.height(),
        pixbuf.rowstride(),
        pixbuf.has_alpha(),
        pixbuf.bits_per_sample(),
        pixbuf.n_channels(),
        bytes.as_ref(),
    )
}

fn pixel_count(width: i32, height: i32) -> Option<usize> {
    if !(1..=MAX_DIMENSION).contains(&width) || !(1..=MAX_DIMENSION).contains(&height) {
        return None;
    }
    usize::try_from(width)
        .ok()?
        .checked_mul(usize::try_from(height).ok()?)
}

fn encode(width: i32, height: i32, rgba: &[u8]) -> Icon {
    Icon {
        width: width as u32,
        height: height as u32,
        rgba: glib::base64_encode(rgba).to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn network_order_argb_preserves_color_and_alpha() {
        let icon = from_argb(2, 1, &[128, 20, 40, 60, 255, 100, 110, 120]).unwrap();
        assert_eq!(
            glib::base64_decode(&icon.rgba),
            [20, 40, 60, 128, 100, 110, 120, 255]
        );
    }

    #[test]
    fn rgb_rows_skip_padding_and_supply_opaque_alpha() {
        let icon = from_rgb(1, 2, 4, false, 8, 3, &[1, 2, 3, 99, 4, 5, 6]).unwrap();
        assert_eq!(
            glib::base64_decode(&icon.rgba),
            [1, 2, 3, 255, 4, 5, 6, 255]
        );
        assert!(from_rgb(2, 2, 4, true, 8, 4, &[0; 16]).is_none());
        assert!(from_rgb(1, 2, 4, true, 8, 4, &[0; 7]).is_none());
    }

    #[test]
    fn icon_dimensions_and_formats_are_bounded_before_decode() {
        assert!(from_argb(0, 1, &[]).is_none());
        assert!(from_argb(513, 1, &[0; 2052]).is_none());
        assert!(from_argb(1, 1, &[0; 3]).is_none());
        assert!(from_rgb(1, 1, 4, true, 16, 4, &[0; 4]).is_none());
        assert!(from_png(b"<svg xmlns='http://www.w3.org/2000/svg'/>").is_none());
    }
}
