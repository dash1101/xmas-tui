//! The relay's side of MQTT 3.1.1, as far as the village uses it: what
//! clients send (connect with a will, subscribe, unsubscribe, publish at
//! QoS 0, ping, disconnect) read from the bytes as they arrive, and what
//! the relay sends back.

/// A packet from a client.
#[derive(Debug, PartialEq)]
pub enum In {
    /// Hello: who, how often it'll be heard from, its will, and a sign-in
    /// (user name, password) if it gave one (the relay's own host does).
    Connect { client: String, keepalive: u16, will: Option<(String, Vec<u8>, bool)>, login: Option<(String, Vec<u8>)> },
    Publish { topic: String, payload: Vec<u8>, retain: bool },
    Subscribe { id: u16, filters: Vec<String> },
    Unsubscribe { id: u16, filters: Vec<String> },
    Ping,
    Disconnect,
    /// Anything else (QoS acks, which the village never needs): ignored.
    Other(u8),
}

/// Bytes in, whole packets out. A packet over `max` bytes, or bytes that
/// aren't MQTT, is an error (the connection is dropped).
pub struct Reader {
    buf: Vec<u8>,
    /// How far into `buf` has been read (what's before it is let go of
    /// once, when the rest runs out, not packet by packet).
    pos: usize,
    max: usize,
}

#[derive(Debug, PartialEq)]
pub enum Bad {
    TooBig,
    Malformed,
}

impl Reader {
    pub fn new(max: usize) -> Self {
        Reader { buf: Vec::new(), pos: 0, max }
    }

    pub fn feed(&mut self, b: &[u8]) -> Result<(), Bad> {
        self.compact();
        if self.buf.len() + b.len() > self.max + 8 {
            return Err(Bad::TooBig);
        }
        self.buf.extend_from_slice(b);
        Ok(())
    }

    /// What's been read, let go of (all at once: reading packet by packet
    /// stays cheap however many come together).
    fn compact(&mut self) {
        if self.pos > 0 {
            self.buf.drain(..self.pos);
            self.pos = 0;
        }
    }

    pub fn next(&mut self) -> Result<Option<In>, Bad> {
        let rest = &self.buf[self.pos..];
        if rest.len() < 2 {
            self.compact();
            return Ok(None);
        }
        let (mut len, mut mul, mut i) = (0usize, 1usize, 1usize);
        loop {
            let Some(&b) = rest.get(i) else { return Ok(None) };
            len += (b & 0x7f) as usize * mul;
            mul *= 128;
            i += 1;
            if b & 0x80 == 0 {
                break;
            }
            if i > 4 {
                return Err(Bad::Malformed);
            }
        }
        if len > self.max {
            return Err(Bad::TooBig);
        }
        if rest.len() < i + len {
            return Ok(None);
        }
        let head = rest[0];
        let out = parse(head, &rest[i..i + len]);
        self.pos += i + len;
        out.map(Some)
    }
}

fn str16(b: &[u8], at: &mut usize) -> Result<String, Bad> {
    let v = bin16(b, at)?;
    String::from_utf8(v).map_err(|_| Bad::Malformed)
}

fn bin16(b: &[u8], at: &mut usize) -> Result<Vec<u8>, Bad> {
    let n = u16_at(b, at)? as usize;
    let v = b.get(*at..*at + n).ok_or(Bad::Malformed)?.to_vec();
    *at += n;
    Ok(v)
}

fn u16_at(b: &[u8], at: &mut usize) -> Result<u16, Bad> {
    let v = b.get(*at..*at + 2).ok_or(Bad::Malformed)?;
    *at += 2;
    Ok(u16::from_be_bytes([v[0], v[1]]))
}

fn parse(head: u8, b: &[u8]) -> Result<In, Bad> {
    let mut at = 0;
    Ok(match head >> 4 {
        1 => {
            let proto = str16(b, &mut at)?;
            let level = *b.get(at).ok_or(Bad::Malformed)?;
            let flags = *b.get(at + 1).ok_or(Bad::Malformed)?;
            at += 2;
            if proto != "MQTT" || level != 4 {
                return Err(Bad::Malformed);
            }
            let keepalive = u16_at(b, &mut at)?;
            let client = str16(b, &mut at)?;
            let will = if flags & 0x04 != 0 {
                let topic = str16(b, &mut at)?;
                let msg = bin16(b, &mut at)?;
                Some((topic, msg, flags & 0x20 != 0))
            } else {
                None
            };
            // a sign-in: only the relay's own host gives one
            let login = if flags & 0x80 != 0 {
                let user = str16(b, &mut at)?;
                let pass = if flags & 0x40 != 0 { bin16(b, &mut at)? } else { vec![] };
                Some((user, pass))
            } else {
                None
            };
            In::Connect { client, keepalive, will, login }
        }
        3 => {
            let topic = str16(b, &mut at)?;
            let qos = (head >> 1) & 3;
            if qos > 0 {
                u16_at(b, &mut at)?;
            }
            In::Publish { topic, payload: b[at..].to_vec(), retain: head & 1 != 0 }
        }
        8 => {
            let id = u16_at(b, &mut at)?;
            let mut filters = vec![];
            while at < b.len() {
                filters.push(str16(b, &mut at)?);
                at += 1; // (the QoS asked for: 0 is all there is)
            }
            if filters.is_empty() || at > b.len() {
                return Err(Bad::Malformed);
            }
            In::Subscribe { id, filters }
        }
        10 => {
            let id = u16_at(b, &mut at)?;
            let mut filters = vec![];
            while at < b.len() {
                filters.push(str16(b, &mut at)?);
            }
            In::Unsubscribe { id, filters }
        }
        12 => In::Ping,
        14 => In::Disconnect,
        k => In::Other(k),
    })
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

pub fn connack(code: u8) -> Vec<u8> {
    vec![0x20, 2, 0, code]
}

pub fn suback(id: u16, granted: &[u8]) -> Vec<u8> {
    let mut out = vec![0x90];
    varint(&mut out, 2 + granted.len());
    out.extend_from_slice(&id.to_be_bytes());
    out.extend_from_slice(granted);
    out
}

pub fn unsuback(id: u16) -> Vec<u8> {
    let mut out = vec![0xB0, 2];
    out.extend_from_slice(&id.to_be_bytes());
    out
}

pub fn pingresp() -> Vec<u8> {
    vec![0xD0, 0]
}

pub fn publish(topic: &str, payload: &[u8], retain: bool) -> Vec<u8> {
    let mut out = vec![0x30 | retain as u8];
    varint(&mut out, 2 + topic.len() + payload.len());
    out.extend_from_slice(&(topic.len() as u16).to_be_bytes());
    out.extend_from_slice(topic.as_bytes());
    out.extend_from_slice(payload);
    out
}

/// Does `topic` match the subscription `filter` (`+` one level, `#` the
/// rest)?
#[allow(dead_code)]
pub fn matches(filter: &str, topic: &str) -> bool {
    let mut f = filter.split('/');
    let mut t = topic.split('/');
    loop {
        match (f.next(), t.next()) {
            (Some("#"), _) => return true,
            (Some("+"), Some(_)) => {}
            (Some(a), Some(b)) if a == b => {}
            (None, None) => return true,
            _ => return false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn str16_out(out: &mut Vec<u8>, s: &[u8]) {
        out.extend_from_slice(&(s.len() as u16).to_be_bytes());
        out.extend_from_slice(s);
    }

    /// A connect as the village sends it (see the game's src/net/mqtt.rs).
    fn connect(client: &str, will: Option<(&str, &[u8])>) -> Vec<u8> {
        let mut v = vec![];
        str16_out(&mut v, b"MQTT");
        v.push(4);
        v.push(0x02 | if will.is_some() { 0x04 | 0x20 } else { 0 });
        v.extend_from_slice(&45u16.to_be_bytes());
        str16_out(&mut v, client.as_bytes());
        if let Some((t, p)) = will {
            str16_out(&mut v, t.as_bytes());
            str16_out(&mut v, p);
        }
        let mut out = vec![0x10];
        varint(&mut out, v.len());
        out.extend(v);
        out
    }

    #[test]
    fn reads_what_the_village_sends_however_it_arrives() {
        let mut bytes = connect("xt1234", Some(("xmas-tui/1/here/ab", b"bye")));
        let mut sub = vec![0, 7];
        for f in ["xmas-tui/1/mail/aa", "xmas-tui/1/sync/bb/+"] {
            str16_out(&mut sub, f.as_bytes());
            sub.push(0);
        }
        bytes.push(0x82);
        varint(&mut bytes, sub.len());
        bytes.extend(sub);
        bytes.extend(publish("xmas-tui/1/here/ab", &[9; 300], true));
        bytes.extend([0xC0, 0, 0xE0, 0]);
        let mut r = Reader::new(1 << 16);
        let mut got = vec![];
        for b in bytes {
            r.feed(&[b]).unwrap();
            while let Some(p) = r.next().unwrap() {
                got.push(p);
            }
        }
        assert_eq!(got[0], In::Connect { client: "xt1234".into(), keepalive: 45, will: Some(("xmas-tui/1/here/ab".into(), b"bye".to_vec(), true)), login: None });
        assert_eq!(got[1], In::Subscribe { id: 7, filters: vec!["xmas-tui/1/mail/aa".into(), "xmas-tui/1/sync/bb/+".into()] });
        assert_eq!(got[2], In::Publish { topic: "xmas-tui/1/here/ab".into(), payload: vec![9; 300], retain: true });
        assert_eq!(got[3..], [In::Ping, In::Disconnect]);
    }

    #[test]
    fn too_big_or_not_mqtt_is_refused() {
        let mut r = Reader::new(100);
        r.feed(&publish("t", &[0; 200], false)).unwrap_err();
        let mut r = Reader::new(100);
        r.feed(&[0x30, 0xff, 0xff, 0xff, 0xff, 0x01]).unwrap();
        assert!(r.next().is_err());
        let mut r = Reader::new(1000);
        r.feed(&[0x30, 0x80, 0x10]).unwrap();
        assert_eq!(r.next(), Err(Bad::TooBig));
    }

    #[test]
    fn filters_match_as_mqtt_says() {
        assert!(matches("xmas-tui/1/sync/ab/+", "xmas-tui/1/sync/ab/dev1"));
        assert!(!matches("xmas-tui/1/sync/ab/+", "xmas-tui/1/sync/ab/dev1/x"));
        assert!(matches("xmas-tui/1/#", "xmas-tui/1/here/x"));
        assert!(matches("xmas-tui/1/here/x", "xmas-tui/1/here/x"));
        assert!(!matches("xmas-tui/1/here/x", "xmas-tui/1/here/y"));
        assert!(!matches("xmas-tui/1/here", "xmas-tui/1/here/y"));
    }

    /// Many tiny packets together are read in a moment, however many (each
    /// one doesn't move all the rest along).
    #[test]
    fn many_small_packets_are_read_quickly() {
        let mut r = Reader::new(1 << 20);
        let bytes: Vec<u8> = std::iter::repeat([0xf0u8, 0x00]).take(400_000).flatten().collect();
        let t = std::time::Instant::now();
        r.feed(&bytes).unwrap();
        let mut n = 0;
        while let Ok(Some(_)) = r.next() {
            n += 1;
        }
        assert_eq!(n, 400_000);
        assert!(t.elapsed().as_millis() < 500, "{:?}", t.elapsed());
        // and what's left after is kept for the next bytes
        r.feed(&[0xc0]).unwrap();
        assert_eq!(r.next(), Ok(None));
        r.feed(&[0x00]).unwrap();
        assert_eq!(r.next(), Ok(Some(In::Ping)));
    }
}
