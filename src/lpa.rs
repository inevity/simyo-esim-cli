//! LPA string construction and terminal QR rendering.

use qrcode::render::unicode::Dense1x2;
use qrcode::QrCode;

/// Build the LPA activation string from the raw activation code.
pub fn build_lpa(activation_code: &str) -> String {
    format!("LPA:{activation_code}")
}

/// Render `data` as a Unicode half-block QR suitable for terminals.
pub fn render_qr(data: &str) -> Option<String> {
    let code = QrCode::new(data).ok()?;
    Some(code.render::<Dense1x2>().quiet_zone(false).build())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_lpa_prefixes_code() {
        assert_eq!(build_lpa("1$simyo.nl$AC"), "LPA:1$simyo.nl$AC");
        assert_eq!(build_lpa(""), "LPA:");
    }

    #[test]
    fn render_qr_produces_multiline_output() {
        let qr = render_qr("LPA:1$simyo.nl$AC").expect("qr render");
        assert!(!qr.is_empty());
        assert!(
            qr.lines().count() > 4,
            "expected multiple lines, got: {qr:?}"
        );
        assert!(qr.contains('\u{2580}') || qr.contains('\u{2584}') || qr.contains(' '));
    }

    #[test]
    fn render_qr_works_for_realistic_activation_code() {
        let code = "LPA:1$simyo.example.com$abcd-1234-EFGH";
        let qr = render_qr(code).expect("qr render for realistic code");
        assert!(qr.lines().count() > 4);
    }
}
