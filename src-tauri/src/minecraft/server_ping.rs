//! Minecraft's "Server List Ping" protocol - the same handshake the vanilla
//! client uses to show MOTD/player count/icon in its own server list, per
//! https://wiki.vg/Server_List_Ping. Only supports the modern (1.7+)
//! handshake-based ping; older servers won't respond and will just surface
//! as unreachable.

use serde::Serialize;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerStatus {
    /// The MOTD as a sequence of independently-styled runs, in order - the
    /// frontend renders each as its own <span>, reproducing the colors and
    /// styles the vanilla client's own server list would show.
    pub motd: Vec<TextRun>,
    pub online: Option<u32>,
    pub max: Option<u32>,
    /// Already a full `data:image/png;base64,...` string per the protocol -
    /// usable directly as an `<img>` src with no further work.
    pub favicon: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TextRun {
    pub text: String,
    pub color: Option<String>,
    pub bold: bool,
    pub italic: bool,
    pub underlined: bool,
    pub strikethrough: bool,
}

pub async fn ping(address: &str) -> anyhow::Result<ServerStatus> {
    let (host, port) = match address.rsplit_once(':') {
        Some((h, p)) => match p.parse::<u16>() {
            Ok(port) => (h.to_string(), port),
            Err(_) => (address.to_string(), 25565),
        },
        None => (address.to_string(), 25565),
    };

    tokio::time::timeout(Duration::from_secs(5), ping_inner(&host, port))
        .await
        .map_err(|_| anyhow::anyhow!("Timed out waiting for a response"))?
}

async fn ping_inner(host: &str, port: u16) -> anyhow::Result<ServerStatus> {
    let mut stream = TcpStream::connect((host, port)).await?;

    let mut handshake_data = Vec::new();
    write_varint(&mut handshake_data, -1); // protocol version - -1 works for a status-only ping
    write_string(&mut handshake_data, host);
    handshake_data.extend_from_slice(&port.to_be_bytes());
    write_varint(&mut handshake_data, 1); // next state: status
    write_packet(&mut stream, 0x00, &handshake_data).await?;
    write_packet(&mut stream, 0x00, &[]).await?; // status request, empty body

    let _packet_len = read_varint(&mut stream).await?;
    let packet_id = read_varint(&mut stream).await?;
    if packet_id != 0 {
        anyhow::bail!("Unexpected response from server");
    }
    let json_len = read_varint(&mut stream).await? as usize;
    let mut json_buf = vec![0u8; json_len];
    stream.read_exact(&mut json_buf).await?;
    let json_str = String::from_utf8(json_buf)?;
    let value: serde_json::Value = serde_json::from_str(&json_str)?;

    let mut motd = Vec::new();
    if let Some(description) = value.get("description") {
        flatten_chat_component(description, Style::default(), &mut motd);
    }
    let online = value
        .get("players")
        .and_then(|p| p.get("online"))
        .and_then(|v| v.as_u64())
        .map(|v| v as u32);
    let max = value
        .get("players")
        .and_then(|p| p.get("max"))
        .and_then(|v| v.as_u64())
        .map(|v| v as u32);
    let favicon = value.get("favicon").and_then(|v| v.as_str()).map(|s| s.to_string());

    Ok(ServerStatus { motd, online, max, favicon })
}

#[derive(Debug, Clone, Default)]
struct Style {
    color: Option<String>,
    bold: bool,
    italic: bool,
    underlined: bool,
    strikethrough: bool,
}

fn named_color_to_hex(name: &str) -> Option<&'static str> {
    Some(match name {
        "black" => "#000000",
        "dark_blue" => "#0000AA",
        "dark_green" => "#00AA00",
        "dark_aqua" => "#00AAAA",
        "dark_red" => "#AA0000",
        "dark_purple" => "#AA00AA",
        "gold" => "#FFAA00",
        "gray" => "#AAAAAA",
        "dark_gray" => "#555555",
        "blue" => "#5555FF",
        "green" => "#55FF55",
        "aqua" => "#55FFFF",
        "red" => "#FF5555",
        "light_purple" => "#FF55FF",
        "yellow" => "#FFFF55",
        "white" => "#FFFFFF",
        _ => return None,
    })
}

/// Legacy `§`-prefixed codes (colors `0-9a-f`, styles `k`/`l`/`m`/`n`/`o`,
/// and `r` to reset) - the same ones vanilla Minecraft chat/MOTDs use,
/// often embedded directly in an otherwise plain-string description.
fn apply_legacy_code(code: char, style: &mut Style) {
    if let Some(hex) = named_color_to_hex(match code {
        '0' => "black",
        '1' => "dark_blue",
        '2' => "dark_green",
        '3' => "dark_aqua",
        '4' => "dark_red",
        '5' => "dark_purple",
        '6' => "gold",
        '7' => "gray",
        '8' => "dark_gray",
        '9' => "blue",
        'a' => "green",
        'b' => "aqua",
        'c' => "red",
        'd' => "light_purple",
        'e' => "yellow",
        'f' => "white",
        _ => "",
    }) {
        style.color = Some(hex.to_string());
        return;
    }
    match code {
        'l' => style.bold = true,
        'o' => style.italic = true,
        'n' => style.underlined = true,
        'm' => style.strikethrough = true,
        'r' => *style = Style::default(),
        _ => {}
    }
}

/// Splits a string into styled runs, starting from `base` and updating the
/// running style whenever a `§` code is encountered.
fn parse_legacy_text(text: &str, base: Style, out: &mut Vec<TextRun>) {
    let mut style = base;
    let mut current = String::new();
    let mut chars = text.chars();

    while let Some(c) = chars.next() {
        if c == '§' {
            if let Some(code) = chars.next() {
                if !current.is_empty() {
                    out.push(to_run(std::mem::take(&mut current), &style));
                }
                apply_legacy_code(code.to_ascii_lowercase(), &mut style);
            }
        } else {
            current.push(c);
        }
    }
    if !current.is_empty() {
        out.push(to_run(current, &style));
    }
}

fn to_run(text: String, style: &Style) -> TextRun {
    TextRun {
        text,
        color: style.color.clone(),
        bold: style.bold,
        italic: style.italic,
        underlined: style.underlined,
        strikethrough: style.strikethrough,
    }
}

/// The MOTD ("description") is a chat component - either a plain string
/// (which may itself contain legacy `§` codes), or an object with its own
/// `color`/`bold`/etc. fields, a `text` field, and a recursive `extra` array
/// of more components that inherit its resolved style. Flattens the whole
/// tree into an ordered list of independently-styled runs.
fn flatten_chat_component(value: &serde_json::Value, base: Style, out: &mut Vec<TextRun>) {
    match value {
        serde_json::Value::String(s) => parse_legacy_text(s, base, out),
        serde_json::Value::Array(items) => {
            for item in items {
                flatten_chat_component(item, base.clone(), out);
            }
        }
        serde_json::Value::Object(_) => {
            let mut style = base;
            if let Some(color) = value.get("color").and_then(|v| v.as_str()) {
                if let Some(hex) = color.strip_prefix('#') {
                    style.color = Some(format!("#{hex}"));
                } else if let Some(hex) = named_color_to_hex(color) {
                    style.color = Some(hex.to_string());
                }
            }
            if value.get("bold").and_then(|v| v.as_bool()) == Some(true) {
                style.bold = true;
            }
            if value.get("italic").and_then(|v| v.as_bool()) == Some(true) {
                style.italic = true;
            }
            if value.get("underlined").and_then(|v| v.as_bool()) == Some(true) {
                style.underlined = true;
            }
            if value.get("strikethrough").and_then(|v| v.as_bool()) == Some(true) {
                style.strikethrough = true;
            }

            if let Some(text) = value.get("text").and_then(|v| v.as_str()) {
                parse_legacy_text(text, style.clone(), out);
            }
            if let Some(extra) = value.get("extra").and_then(|v| v.as_array()) {
                for item in extra {
                    flatten_chat_component(item, style.clone(), out);
                }
            }
        }
        _ => {}
    }
}

fn write_varint(buf: &mut Vec<u8>, mut value: i32) {
    loop {
        let mut byte = (value & 0x7F) as u8;
        value = ((value as u32) >> 7) as i32;
        if value != 0 {
            byte |= 0x80;
        }
        buf.push(byte);
        if value == 0 {
            break;
        }
    }
}

fn write_string(buf: &mut Vec<u8>, s: &str) {
    write_varint(buf, s.len() as i32);
    buf.extend_from_slice(s.as_bytes());
}

async fn write_packet(stream: &mut TcpStream, packet_id: i32, data: &[u8]) -> anyhow::Result<()> {
    let mut body = Vec::new();
    write_varint(&mut body, packet_id);
    body.extend_from_slice(data);

    let mut packet = Vec::new();
    write_varint(&mut packet, body.len() as i32);
    packet.extend_from_slice(&body);

    stream.write_all(&packet).await?;
    Ok(())
}

async fn read_varint(stream: &mut TcpStream) -> anyhow::Result<i32> {
    let mut value: i32 = 0;
    let mut position = 0;
    loop {
        let mut byte = [0u8; 1];
        stream.read_exact(&mut byte).await?;
        let byte = byte[0];
        value |= ((byte & 0x7F) as i32) << position;
        if (byte & 0x80) == 0 {
            break;
        }
        position += 7;
        if position >= 32 {
            anyhow::bail!("VarInt too large");
        }
    }
    Ok(value)
}
