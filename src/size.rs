//! Conversão entre tamanhos legíveis (`10K`, `1.5MiB`) e bytes. As unidades são
//! binárias (1 K = 1024 bytes), como em `du -h` e `ls -lh`.

const UNITS: [&str; 5] = ["KiB", "MiB", "GiB", "TiB", "PiB"];

/// Lê um tamanho como `512`, `10K`, `1.5M`, `2GiB` ou `100 kb` (sem diferenciar maiúsculas).
pub fn parse_size(input: &str) -> Result<u64, String> {
    let text = input.trim();
    let split = text
        .find(|c: char| !(c.is_ascii_digit() || c == '.'))
        .unwrap_or(text.len());
    let (number, unit) = text.split_at(split);

    if number.is_empty() {
        return Err(format!(
            "invalid size `{input}`: expected a number such as 512, 10K or 1.5M"
        ));
    }
    let value: f64 = number
        .parse()
        .map_err(|_| format!("invalid size `{input}`: `{number}` is not a number"))?;

    let multiplier: u64 = match unit.trim().to_ascii_lowercase().as_str() {
        "" | "b" => 1,
        "k" | "kb" | "kib" => 1 << 10,
        "m" | "mb" | "mib" => 1 << 20,
        "g" | "gb" | "gib" => 1 << 30,
        "t" | "tb" | "tib" => 1 << 40,
        other => return Err(format!("invalid size `{input}`: unknown unit `{other}`")),
    };

    let bytes = (value * multiplier as f64).round();
    if bytes > u64::MAX as f64 {
        return Err(format!("invalid size `{input}`: too large"));
    }
    Ok(bytes as u64)
}

/// Formata bytes para leitura humana: `512 B`, `1.5 KiB`, `2.0 GiB`.
pub fn format_size(bytes: u64) -> String {
    if bytes < 1024 {
        return format!("{bytes} B");
    }
    let mut value = bytes as f64 / 1024.0;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    format!("{value:.1} {}", UNITS[unit])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_plain_bytes_and_units() {
        assert_eq!(parse_size("0"), Ok(0));
        assert_eq!(parse_size("512"), Ok(512));
        assert_eq!(parse_size("512B"), Ok(512));
        assert_eq!(parse_size("10K"), Ok(10 * 1024));
        assert_eq!(parse_size("10kb"), Ok(10 * 1024));
        assert_eq!(parse_size("1.5M"), Ok(1024 * 1024 * 3 / 2));
        assert_eq!(parse_size("2GiB"), Ok(2 * 1024 * 1024 * 1024));
        assert_eq!(parse_size(" 100 kib "), Ok(100 * 1024));
        assert_eq!(parse_size("1T"), Ok(1 << 40));
    }

    #[test]
    fn rejects_invalid_sizes() {
        assert!(parse_size("").is_err());
        assert!(parse_size("K").is_err());
        assert!(parse_size("-1").is_err());
        assert!(parse_size("1.2.3").is_err());
        assert!(parse_size("10X").unwrap_err().contains("unknown unit"));
        assert!(
            parse_size("99999999999T")
                .unwrap_err()
                .contains("too large")
        );
    }

    #[test]
    fn formats_sizes_for_humans() {
        assert_eq!(format_size(0), "0 B");
        assert_eq!(format_size(1023), "1023 B");
        assert_eq!(format_size(1024), "1.0 KiB");
        assert_eq!(format_size(1536), "1.5 KiB");
        assert_eq!(format_size(5 * 1024 * 1024), "5.0 MiB");
        assert_eq!(format_size(3 << 30), "3.0 GiB");
    }
}
