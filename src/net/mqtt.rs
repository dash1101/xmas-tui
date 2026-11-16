//! Just enough MQTT 3.1.1 to talk through a public broker: connect (with a
//! last will), subscribe and unsubscribe, publish at QoS 0 (retained or
//! not), ping and disconnect; and a reader that picks whole packets out of
//! the bytes as they arrive.

/// A packet from the broker.
#[derive(Debug, PartialEq)]
pub enum Packet {
    /// The answer to connecting (0: accepted).
    ConnAck(u8),
    Publish { topic: String, payload: Vec<u8> },
    SubAck,
    PingResp,
    Other(u8),
}

fn varint(out: &mut Vec<u8>, mut n: usize) {
    loop {
        let mut b = (n % 128) as u8;
        n /= 128;
        if n > 0 {
            b |= 0x80;
        }
        out.push(b);
        if n == 0 {
            break;
        }
    }
}

fn str16(out: &mut Vec<u8>, s: &[u8]) {
    out.extend_from_slice(&(s.len().min(65535) as u16).to_be_bytes());
    out.extend_from_slice(&s[..s.len().min(65535)]);
}

fn packet(head: u8, body: Vec<u8>) -> Vec<u8> {
    let mut out = vec![head];
    varint(&mut out, body.len());
    out.extend(body);
    out
}

/// Connect: a clean session, `keepalive` seconds, and a will (published,
/// retained, if the connection drops without a goodbye).
pub fn connect(client: &str, keepalive: u16, will: Option<(&str, &[u8])>) -> Vec<u8> {
    let mut v = vec![];
    str16(&mut v, b"MQTT");
    v.push(4);
    v.push(0x02 | if will.is_some() { 0x04 | 0x20 } else { 0 });
    v.extend_from_slice(&keepalive.to_be_bytes());
    str16(&mut v, client.as_bytes());
    if let Some((topic, payload)) = will {
        str16(&mut v, topic.as_bytes());
        str16(&mut v, payload);
    }
    packet(0x10, v)
}

pub fn subscribe(id: u16, topics: &[String]) -> Vec<u8> {
    let mut v = id.to_be_bytes().to_vec();
    for t in topics {
        str16(&mut v, t.as_bytes());
        v.push(0);
    }
    packet(0x82, v)
}

pub fn unsubscribe(id: u16, topics: &[String]) -> Vec<u8> {
    let mut v = id.to_be_bytes().to_vec();
    for t in topics {
        str16(&mut v, t.as_bytes());
    }
    packet(0xA2, v)
}

pub fn publish(topic: &str, payload: &[u8], retain: bool) -> Vec<u8> {
    let mut v = vec![];
    str16(&mut v, topic.as_bytes());
    v.extend_from_slice(payload);
    packet(0x30 | retain as u8, v)
}

pub fn ping() -> Vec<u8> {
    vec![0xC0, 0]
}

pub fn disconnect() -> Vec<u8> {
    vec![0xE0, 0]
}

/// Bytes in, whole packets out.
#[derive(Default)]
pub struct Reader {
    buf: Vec<u8>,
}

impl Reader {
    pub fn feed(&mut self, b: &[u8]) {
        self.buf.extend_from_slice(b);
    }

    pub fn clear(&mut self) {
        self.buf.clear();
    }

    /// The next whole packet, if one's in.
    pub fn next(&mut self) -> Option<Packet> {
        if self.buf.len() < 2 {
            return None;
        }
        let (mut len, mut mul, mut i) = (0usize, 1usize, 1usize);
        loop {
            let b = *self.buf.get(i)?;
            len += (b & 0x7f) as usize * mul;
            mul *= 128;
            i += 1;
            if b & 0x80 == 0 {
                break;
            }
            if i > 4 {
                // not MQTT: start again
                self.buf.clear();
                return None;
            }
        }
        if self.buf.len() < i + len {
            return None;
        }
        let head = self.buf[0];
        let body: Vec<u8> = self.buf[i..i + len].to_vec();
        self.buf.drain(..i + len);
        Some(match head >> 4 {
            2 => Packet::ConnAck(body.get(1).copied().unwrap_or(0xff)),
            3 => {
                if body.len() < 2 {
                    return Some(Packet::Other(3));
                }
                let tl = u16::from_be_bytes([body[0], body[1]]) as usize;
                let qos = (head >> 1) & 3;
                let start = 2 + tl + if qos > 0 { 2 } else { 0 };
                if body.len() < start {
                    return Some(Packet::Other(3));
                }
                let topic = String::from_utf8_lossy(&body[2..2 + tl]).to_string();
                Packet::Publish { topic, payload: body[start..].to_vec() }
            }
            9 => Packet::SubAck,
            13 => Packet::PingResp,
            k => Packet::Other(k),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packets_come_back_whole_however_they_arrive() {
        let big = vec![7u8; 300];
        let mut bytes = publish("xmas-tui/1/here/abc", &big, true);
        bytes.extend(vec![0x20, 2, 0, 0]);
        bytes.extend(publish("t", b"hi", false));
        let mut r = Reader::default();
        let mut got = vec![];
        // a byte at a time
        for b in bytes {
            r.feed(&[b]);
            while let Some(p) = r.next() {
                got.push(p);
            }
        }
        assert_eq!(got.len(), 3);
        assert_eq!(got[0], Packet::Publish { topic: "xmas-tui/1/here/abc".into(), payload: big });
        assert_eq!(got[1], Packet::ConnAck(0));
        assert_eq!(got[2], Packet::Publish { topic: "t".into(), payload: b"hi".to_vec() });
        // the length of a connect counts its will
        let c = connect("xt1", 60, Some(("w", b"bye")));
        assert_eq!(c[0], 0x10);
        assert_eq!(c[1] as usize, c.len() - 2);
    }
}
