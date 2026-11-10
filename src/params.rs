// The emulators' parameter strings, such as "engine:sdl,button:0,guid:...,port:0".
// Same rules as their ParamPackage: "key:value" pairs joined by commas, ':' ',' '$' escaped as
// $0 $1 $2, and "[empty]" for nothing. A parameter can hold another one, escaped.

const EMPTY: &str = "[empty]";

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Params(Vec<(String, String)>);

fn escape(s: &str) -> String {
    s.replace('$', "$2").replace(',', "$1").replace(':', "$0")
}

fn unescape(s: &str) -> String {
    s.replace("$0", ":").replace("$1", ",").replace("$2", "$")
}

/// How the emulators write a decimal number.
pub fn float(v: f32) -> String {
    format!("{v:.6}")
}

impl Params {
    pub fn parse(text: &str) -> Self {
        let text = text.trim().trim_matches('"');
        if text.is_empty() || text == EMPTY { return Self::default(); }
        Self(text.split(',').filter_map(|pair| {
            let mut parts = pair.split(':');
            let (k, v) = (parts.next()?, parts.next()?);
            if parts.next().is_some() { return None; }
            Some((unescape(k), unescape(v)))
        }).collect())
    }

    pub fn new(pairs: &[(&str, String)]) -> Self {
        let mut p = Self::default();
        for (k, v) in pairs { p.set(k, v.clone()); }
        p
    }

    #[cfg(test)]
    pub fn get(&self, key: &str) -> Option<&str> {
        self.0.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str())
    }

    pub fn set(&mut self, key: &str, value: impl ToString) {
        let value = value.to_string();
        match self.0.iter_mut().find(|(k, _)| k == key) {
            Some(pair) => pair.1 = value,
            None => self.0.push((key.to_string(), value)),
        }
    }

    #[cfg(test)]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn serialize(&self) -> String {
        if self.0.is_empty() { return EMPTY.to_string(); }
        self.0.iter().map(|(k, v)| format!("{}:{}", escape(k), escape(v))).collect::<Vec<_>>().join(",")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_and_written_like_the_emulators() {
        let p = Params::parse("\"engine:sdl,pad:0,button:0,guid:030000005e0400008e02000014017801,port:0\"");
        assert_eq!(p.get("button"), Some("0"));
        assert_eq!(p.get("guid"), Some("030000005e0400008e02000014017801"));
        assert_eq!(p.serialize(), "engine:sdl,pad:0,button:0,guid:030000005e0400008e02000014017801,port:0");
        assert!(Params::parse("[empty]").is_empty());
        assert_eq!(Params::default().serialize(), "[empty]");
        // A stick made of keys holds whole parameters.
        let mut stick = Params::new(&[("engine", "analog_from_button".into())]);
        stick.set("up", Params::new(&[("engine", "keyboard".into()), ("code", "87".into()), ("toggle", "0".into())]).serialize());
        let text = stick.serialize();
        assert_eq!(text, "engine:analog_from_button,up:engine$0keyboard$1code$087$1toggle$00");
        assert_eq!(Params::parse(stick.get("up").unwrap()).get("code"), Some("87"));
        assert_eq!(Params::parse(&text), stick);
        assert_eq!(float(-0.0), "-0.000000");
        assert_eq!(float(0.15), "0.150000");
    }
}
