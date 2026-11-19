//! Wire format for the t1bridge Touch Bar renderer protocol (renderer-selection v1).
//! One SOCK_SEQPACKET datagram = one message. All integers little-endian.
//!
//! Header (16 bytes): "T1HW" | u16 major=1 | u16 msg_type | u32 payload_len | u32 request_id

pub const MAJOR: u16 = 1;

pub const MSG_HELLO: u16 = 0x0001;
pub const MSG_REGISTER_BUFFER: u16 = 0x0002;
pub const MSG_SUBMIT_FRAME: u16 = 0x0003;
pub const MSG_TAP_KEYS: u16 = 0x0004;
pub const MSG_SET_DISPLAY_BRIGHTNESS: u16 = 0x0005;
pub const MSG_SET_KEYBOARD_BACKLIGHT: u16 = 0x0006;
pub const MSG_CANCEL_TOUCH_ID: u16 = 0x0007;
pub const MSG_STEP_DISPLAY_BRIGHTNESS: u16 = 0x0008;
pub const MSG_STEP_KEYBOARD_BACKLIGHT: u16 = 0x0009;

pub const MSG_HELLO_ACK: u16 = 0x8001;
pub const MSG_ACK: u16 = 0x8002;
pub const MSG_ERROR: u16 = 0x8003;
pub const MSG_FRAME_RELEASED: u16 = 0x9001;
pub const MSG_INPUT_FRAME: u16 = 0x9002;

pub const FEATURE_MEMFD: u64 = 0x01;
pub const FEATURE_INPUT: u64 = 0x02;
pub const FEATURE_TAPKEYS: u64 = 0x04;
pub const FEATURE_DISPLAY_BRIGHTNESS: u64 = 0x08;
pub const FEATURE_KEYBOARD_BACKLIGHT: u64 = 0x10;
pub const FEATURE_TOUCH_ID_CANCEL: u64 = 0x20;

pub const PIXEL_FORMAT_XRGB8888: u32 = 1;

/// Linux key codes accepted by TapKeys (the complete allowlist).
pub const KEY_ESC: u16 = 1;
pub const KEY_F1: u16 = 59;
pub const KEY_F11: u16 = 87;
pub const KEY_F12: u16 = 88;

pub fn key_f(n: u8) -> u16 {
    match n {
        1..=10 => KEY_F1 + (n as u16 - 1),
        11 => KEY_F11,
        12 => KEY_F12,
        _ => 0,
    }
}

#[derive(Debug, Clone)]
pub struct Packet {
    pub kind: u16,
    pub req_id: u32,
    pub payload: Vec<u8>,
}

pub fn encode(kind: u16, req_id: u32, payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(16 + payload.len());
    out.extend_from_slice(b"T1HW");
    out.extend_from_slice(&MAJOR.to_le_bytes());
    out.extend_from_slice(&kind.to_le_bytes());
    out.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    out.extend_from_slice(&req_id.to_le_bytes());
    out.extend_from_slice(payload);
    out
}

pub fn decode(buf: &[u8]) -> Option<Packet> {
    if buf.len() < 16 || &buf[0..4] != b"T1HW" {
        return None;
    }
    let major = u16::from_le_bytes(buf[4..6].try_into().ok()?);
    if major != MAJOR {
        return None;
    }
    let kind = u16::from_le_bytes(buf[6..8].try_into().ok()?);
    let len = u32::from_le_bytes(buf[8..12].try_into().ok()?) as usize;
    let req_id = u32::from_le_bytes(buf[12..16].try_into().ok()?);
    if buf.len() != 16 + len {
        return None;
    }
    Some(Packet { kind, req_id, payload: buf[16..].to_vec() })
}

pub fn hello(minor: u16, features: u64, req_id: u32) -> Vec<u8> {
    let mut p = Vec::with_capacity(12);
    p.extend_from_slice(&minor.to_le_bytes());
    p.extend_from_slice(&[0u8; 2]);
    p.extend_from_slice(&features.to_le_bytes());
    encode(MSG_HELLO, req_id, &p)
}

pub struct HelloAck {
    pub minor: u16,
    pub width: u32,
    pub height: u32,
    pub pixel_format: u32,
    pub max_buffers: u32,
}

pub fn parse_hello_ack(p: &Packet) -> Option<HelloAck> {
    if p.kind != MSG_HELLO_ACK || p.payload.len() != 20 {
        return None;
    }
    Some(HelloAck {
        minor: u16::from_le_bytes(p.payload[0..2].try_into().ok()?),
        width: u32::from_le_bytes(p.payload[4..8].try_into().ok()?),
        height: u32::from_le_bytes(p.payload[8..12].try_into().ok()?),
        pixel_format: u32::from_le_bytes(p.payload[12..16].try_into().ok()?),
        max_buffers: u32::from_le_bytes(p.payload[16..20].try_into().ok()?),
    })
}

pub fn register_buffer(buffer_id: u32, stride: u32, byte_len: u64, req_id: u32) -> Vec<u8> {
    let mut p = Vec::with_capacity(16);
    p.extend_from_slice(&buffer_id.to_le_bytes());
    p.extend_from_slice(&stride.to_le_bytes());
    p.extend_from_slice(&byte_len.to_le_bytes());
    encode(MSG_REGISTER_BUFFER, req_id, &p)
}

pub fn submit_frame(buffer_id: u32, frame_id: u64, req_id: u32) -> Vec<u8> {
    let mut p = Vec::with_capacity(16);
    p.extend_from_slice(&buffer_id.to_le_bytes());
    p.extend_from_slice(&frame_id.to_le_bytes());
    p.extend_from_slice(&0u32.to_le_bytes()); // damage_count=0 => full frame
    encode(MSG_SUBMIT_FRAME, req_id, &p)
}

pub fn tap_keys(keys: &[u16], req_id: u32) -> Option<Vec<u8>> {
    if keys.is_empty() || keys.len() > 4 || keys.iter().any(|k| *k == 0) {
        return None;
    }
    let mut p = Vec::with_capacity(12);
    p.push(keys.len() as u8);
    p.extend_from_slice(&[0u8; 3]);
    for slot in 0..4 {
        let k = keys.get(slot).copied().unwrap_or(0);
        p.extend_from_slice(&k.to_le_bytes());
    }
    Some(encode(MSG_TAP_KEYS, req_id, &p))
}

pub fn step_display(up: bool, req_id: u32) -> Vec<u8> {
    encode(MSG_STEP_DISPLAY_BRIGHTNESS, req_id, &[if up { 1 } else { 0 }])
}

pub fn step_keyboard(up: bool, req_id: u32) -> Vec<u8> {
    encode(MSG_STEP_KEYBOARD_BACKLIGHT, req_id, &[if up { 1 } else { 0 }])
}

#[derive(Debug, Clone, Copy)]
pub struct Contact {
    pub id: u8,
    pub tip: bool,
    pub in_range: bool,
    pub x: u32,
    pub y: u32,
}

pub struct InputFrame {
    pub fn_pressed: bool,
    pub contacts: Vec<Contact>,
}

pub fn parse_input_frame(p: &Packet) -> Option<InputFrame> {
    if p.kind != MSG_INPUT_FRAME || p.payload.len() < 12 {
        return None;
    }
    let fn_pressed = p.payload[8] != 0;
    let count = p.payload[9] as usize;
    if count > 10 || p.payload.len() != 12 + count * 12 {
        return None;
    }
    let mut contacts = Vec::with_capacity(count);
    for i in 0..count {
        let o = 12 + i * 12;
        contacts.push(Contact {
            id: p.payload[o],
            tip: p.payload[o + 1] != 0,
            in_range: p.payload[o + 2] != 0,
            x: u32::from_le_bytes(p.payload[o + 4..o + 8].try_into().ok()?),
            y: u32::from_le_bytes(p.payload[o + 8..o + 12].try_into().ok()?),
        });
    }
    Some(InputFrame { fn_pressed, contacts })
}

pub struct FrameReleased {
    pub buffer_id: u32,
    pub frame_id: u64,
}

pub fn parse_frame_released(p: &Packet) -> Option<FrameReleased> {
    if p.kind != MSG_FRAME_RELEASED || p.payload.len() != 12 {
        return None;
    }
    Some(FrameReleased {
        buffer_id: u32::from_le_bytes(p.payload[0..4].try_into().ok()?),
        frame_id: u64::from_le_bytes(p.payload[4..12].try_into().ok()?),
    })
}
