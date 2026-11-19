use std::io::{self, Write};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use potoo_lse_interpreter::{
    Host, Interpreter, StoredElementKind, StoredScalar, StoredValue, parse,
};

struct CliHost {
    state: u64,
}
impl Host for CliHost {
    fn input(&mut self) -> Result<String, String> {
        let mut line = String::new();
        io::stdin()
            .read_line(&mut line)
            .map_err(|e| e.to_string())?;
        Ok(line.trim_end_matches(['\r', '\n']).to_owned())
    }
    fn output(&mut self, text: &str) -> Result<(), String> {
        print!("{text}");
        io::stdout().flush().map_err(|e| e.to_string())
    }
    fn newline(&mut self, count: usize) -> Result<(), String> {
        for _ in 0..count {
            println!();
        }
        Ok(())
    }
    fn pause(&mut self, line: i64) -> Result<(), String> {
        eprintln!("PAUSE at LSE line {line}; press Enter to continue");
        let mut input = String::new();
        io::stdin()
            .read_line(&mut input)
            .map_err(|e| e.to_string())?;
        Ok(())
    }
    fn date_time(&mut self) -> Result<String, String> {
        let secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_secs();
        let days = (secs / 86_400) as i64;
        let second = secs % 86_400;
        let z = days + 719_468;
        let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
        let doe = z - era * 146_097;
        let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
        let mut year = yoe + era * 400;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let day = doy - (153 * mp + 2) / 5 + 1;
        let month = mp + if mp < 10 { 3 } else { -9 };
        year += if month <= 2 { 1 } else { 0 };
        Ok(format!(
            "{day:02}/{month:02}/{:02} {:02}:{:02}:{:02}",
            year.rem_euclid(100),
            second / 3_600,
            (second % 3_600) / 60,
            second % 60
        ))
    }
    fn random_unit(&mut self) -> Result<f64, String> {
        self.state ^= self.state << 13;
        self.state ^= self.state >> 7;
        self.state ^= self.state << 17;
        let n = (self.state as f64 + 0.5) / (u64::MAX as f64 + 1.0);
        Ok(n.clamp(f64::MIN_POSITIVE, 1.0 - f64::EPSILON))
    }
    fn store_record(&mut self, file: &str, key: f64, value: StoredValue) -> Result<i64, String> {
        if !key.is_finite() {
            return Err("record keys must be finite numbers".into());
        }
        let mut records = read_records(file)?;
        if let Some((_, stored)) = records
            .iter_mut()
            .find(|(stored_key, _)| *stored_key == key)
        {
            *stored = value;
        } else {
            records.push((key, value));
        }
        write_records(file, &records)?;
        Ok(0)
    }
    fn load_record(&mut self, file: &str, key: f64) -> Result<(i64, Option<StoredValue>), String> {
        if !key.is_finite() {
            return Err("record keys must be finite numbers".into());
        }
        let records = read_records(file)?;
        Ok(
            match records
                .into_iter()
                .find(|(stored_key, _)| *stored_key == key)
            {
                Some((_, value)) => (0, Some(value)),
                None => (1, None),
            },
        )
    }
    fn delete_records(&mut self, file: &str, key: Option<f64>) -> Result<i64, String> {
        let Some(key) = key else {
            return match std::fs::remove_file(file) {
                Ok(()) => Ok(0),
                Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(1),
                Err(error) => Err(error.to_string()),
            };
        };
        if !key.is_finite() {
            return Err("record keys must be finite numbers".into());
        }
        let mut records = read_records(file)?;
        let original_len = records.len();
        records.retain(|(stored_key, _)| *stored_key != key);
        if records.len() == original_len {
            return Ok(1);
        }
        write_records(file, &records)?;
        Ok(0)
    }
    fn load_procedure_source(&mut self, name: &str) -> Result<Option<String>, String> {
        let path = format!("{name}.lse");
        match std::fs::read_to_string(path) {
            Ok(source) => Ok(Some(source)),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error.to_string()),
        }
    }
}

fn read_records(path: &str) -> Result<Vec<(f64, StoredValue)>, String> {
    let contents = match std::fs::read_to_string(path) {
        Ok(contents) => contents,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error.to_string()),
    };
    contents
        .lines()
        .map(|line| {
            let (key, value) = line
                .split_once('\t')
                .ok_or_else(|| "invalid LSE record file".to_owned())?;
            let bits = u64::from_str_radix(key, 16).map_err(|error| error.to_string())?;
            let key = f64::from_bits(bits);
            if !key.is_finite() {
                return Err("record file contains a non-finite key".into());
            }
            Ok((key, decode_value(value)?))
        })
        .collect()
}

fn write_records(path: &str, records: &[(f64, StoredValue)]) -> Result<(), String> {
    let mut contents = String::new();
    for (key, value) in records {
        contents.push_str(&format!(
            "{:016x}\t{}\n",
            key.to_bits(),
            encode_value(value)
        ));
    }
    std::fs::write(Path::new(path), contents).map_err(|error| error.to_string())
}

fn encode_value(value: &StoredValue) -> String {
    match value {
        StoredValue::Real(number) => format!("R\t{:016x}", number.to_bits()),
        StoredValue::Text(text) => format!("T\t{}", hex_encode(text.as_bytes())),
        StoredValue::Boolean(value) => format!("B\t{}", u8::from(*value)),
        StoredValue::Array {
            dimensions,
            element_kind,
            elements,
        } => {
            let kind = match element_kind {
                StoredElementKind::Numeric => "R",
                StoredElementKind::Text => "T",
                StoredElementKind::Boolean => "B",
            };
            let dimensions = dimensions
                .iter()
                .map(usize::to_string)
                .collect::<Vec<_>>()
                .join(",");
            let elements = elements
                .iter()
                .map(encode_scalar)
                .collect::<Vec<_>>()
                .join(";");
            format!("A\t{kind}\t{dimensions}\t{elements}")
        }
    }
}

fn decode_value(encoded: &str) -> Result<StoredValue, String> {
    let mut fields = encoded.split('\t');
    match fields.next() {
        Some("R") => {
            let bits = u64::from_str_radix(fields.next().ok_or("missing number")?, 16)
                .map_err(|error| error.to_string())?;
            Ok(StoredValue::Real(f64::from_bits(bits)))
        }
        Some("T") => {
            let bytes = hex_decode(fields.next().ok_or("missing string")?)?;
            let text = String::from_utf8(bytes).map_err(|error| error.to_string())?;
            Ok(StoredValue::Text(text))
        }
        Some("B") => match fields.next() {
            Some("0") => Ok(StoredValue::Boolean(false)),
            Some("1") => Ok(StoredValue::Boolean(true)),
            _ => Err("invalid Boolean record".into()),
        },
        Some("A") => {
            let element_kind = match fields.next() {
                Some("R") => StoredElementKind::Numeric,
                Some("T") => StoredElementKind::Text,
                Some("B") => StoredElementKind::Boolean,
                _ => return Err("invalid table element type".into()),
            };
            let dimensions = fields
                .next()
                .ok_or("missing table dimensions")?
                .split(',')
                .map(|dimension| {
                    dimension
                        .parse::<usize>()
                        .map_err(|error| error.to_string())
                })
                .collect::<Result<Vec<_>, _>>()?;
            let encoded_elements = fields.next().ok_or("missing table elements")?;
            let elements = if encoded_elements.is_empty() {
                Vec::new()
            } else {
                encoded_elements
                    .split(';')
                    .map(decode_scalar)
                    .collect::<Result<Vec<_>, _>>()?
            };
            Ok(StoredValue::Array {
                dimensions,
                element_kind,
                elements,
            })
        }
        _ => Err("invalid LSE record type".into()),
    }
}

fn encode_scalar(value: &StoredScalar) -> String {
    match value {
        StoredScalar::Real(number) => format!("R{:016x}", number.to_bits()),
        StoredScalar::Text(text) => format!("T{}", hex_encode(text.as_bytes())),
        StoredScalar::Boolean(value) => format!("B{}", u8::from(*value)),
    }
}

fn decode_scalar(encoded: &str) -> Result<StoredScalar, String> {
    let (kind, value) = encoded.split_at_checked(1).ok_or("empty table element")?;
    match kind {
        "R" => {
            let bits = u64::from_str_radix(value, 16).map_err(|error| error.to_string())?;
            Ok(StoredScalar::Real(f64::from_bits(bits)))
        }
        "T" => {
            let bytes = hex_decode(value)?;
            let text = String::from_utf8(bytes).map_err(|error| error.to_string())?;
            Ok(StoredScalar::Text(text))
        }
        "B" => match value {
            "0" => Ok(StoredScalar::Boolean(false)),
            "1" => Ok(StoredScalar::Boolean(true)),
            _ => Err("invalid Boolean table element".into()),
        },
        _ => Err("invalid table element type".into()),
    }
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        encoded.push(HEX[(byte >> 4) as usize] as char);
        encoded.push(HEX[(byte & 0x0f) as usize] as char);
    }
    encoded
}

fn hex_decode(encoded: &str) -> Result<Vec<u8>, String> {
    if !encoded.is_ascii() {
        return Err("invalid hexadecimal string".into());
    }
    if !encoded.len().is_multiple_of(2) {
        return Err("invalid hexadecimal string".into());
    }
    (0..encoded.len())
        .step_by(2)
        .map(|index| {
            u8::from_str_radix(&encoded[index..index + 2], 16).map_err(|error| error.to_string())
        })
        .collect()
}

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
fn run() -> Result<(), String> {
    let path = std::env::args()
        .nth(1)
        .ok_or_else(|| "usage: potoo-lse <source-file>".to_owned())?;
    let source = std::fs::read_to_string(&path).map_err(|e| format!("{path}: {e}"))?;
    let program = parse(&source).map_err(|e| {
        e.iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n")
    })?;
    let seed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_nanos() as u64
        | 1;
    Interpreter::new(CliHost { state: seed })
        .run(&program)
        .map_err(|e| e.to_string())?;
    Ok(())
}
