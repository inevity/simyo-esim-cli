//! Pure parsing / validation helpers for Simyo API responses (no I/O).
//! Mirrors the defensive parsing in eSIM-Tools `src/simyo/js/modules/`.

use serde_json::Value;

/// MFA states after which the session token is immediately usable.
pub const MFA_SKIP_STATUSES: &[&str] = &[
    "DISABLED_BY_CUSTOMER",
    "DISABLED",
    "NONE",
    "OFF",
    "OK",
    "VERIFIED",
    "COMPLETED",
    "NOT_REQUIRED",
    "INACTIVE",
    "DISABLED_BY_OPERATOR",
];

/// MFA states that require an OTP round-trip via `security.verifyOTP`.
pub const MFA_PENDING_STATUSES: &[&str] = &[
    "PENDING_VERIFICATION",
    "PENDING",
    "REQUIRED",
    "CHALLENGE_REQUIRED",
    "ENABLED",
    "ACTIVE",
];

/// eSIM order status: validation code sent, waiting for user input.
pub const ESIM_WAITING_FOR_VALIDATION_CODE: &str = "ESIM_REQUEST_WAITING_FOR_VALIDATION_CODE";

/// eSIM order status: order start requested (ordering is safe / expected next).
pub const ESIM_START_REQUEST: &str = "ESIM_START_REQUEST";

/// eSIM order status: profile ready to download.
pub const ESIM_READY_FOR_DOWNLOAD: &str = "ESIM_REQUEST_READY_FOR_DOWNLOAD";

/// True when the account has no login MFA or it is already satisfied.
pub fn is_mfa_skip(mfa_status: Option<&str>) -> bool {
    match mfa_status {
        None => true,
        Some(raw) => {
            let s = raw.trim().to_uppercase();
            s.is_empty() || MFA_SKIP_STATUSES.contains(&s.as_str())
        }
    }
}

/// True when login returns a temporary session that requires an OTP.
pub fn is_mfa_pending(mfa_status: Option<&str>) -> bool {
    match mfa_status {
        None => false,
        Some(raw) => {
            if is_mfa_skip(Some(raw)) {
                return false;
            }
            let s = raw.trim().to_uppercase();
            if MFA_PENDING_STATUSES.contains(&s.as_str()) {
                return true;
            }
            if s.contains("PENDING") || s.contains("CHALLENGE") {
                return true;
            }
            s.contains("REQUIRED") && !s.contains("NOT_REQUIRED")
        }
    }
}

fn result_obj(root: &Value) -> Option<&Value> {
    root.get("result").filter(|v| v.is_object())
}

fn str_field(obj: &Value, key: &str) -> Option<String> {
    obj.get(key)
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

/// Extract `result.sessionToken` from a login response.
pub fn extract_session_token(root: &Value) -> Option<String> {
    result_obj(root).and_then(|r| str_field(r, "sessionToken"))
}

/// Extract the formal session token from a `verifyOTP` response
/// (`result.token` preferred, `result.sessionToken` fallback).
pub fn extract_formal_token(root: &Value) -> Option<String> {
    result_obj(root).and_then(|r| str_field(r, "token").or_else(|| str_field(r, "sessionToken")))
}

pub fn extract_mfa_status(root: &Value) -> Option<String> {
    result_obj(root).and_then(|r| str_field(r, "mfaStatus"))
}

pub fn extract_mfa_method(root: &Value) -> Option<String> {
    result_obj(root).and_then(|r| str_field(r, "mfaMethod"))
}

/// Parsed eSIM info; `activation_code` is mandatory, the rest is best-effort.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EsimInfo {
    pub activation_code: String,
    pub status: Option<String>,
    pub phone_number: Option<String>,
    pub iccid: Option<String>,
}

pub fn extract_activation_code(root: &Value) -> Option<String> {
    result_obj(root).and_then(|r| str_field(r, "activationCode"))
}

/// Extract eSIM info; returns `None` when `activationCode` is absent.
pub fn extract_esim_info(root: &Value) -> Option<EsimInfo> {
    let r = result_obj(root)?;
    let activation_code = extract_activation_code(root)?;
    Some(EsimInfo {
        activation_code,
        status: str_field(r, "status"),
        phone_number: str_field(r, "phoneNumber"),
        iccid: str_field(r, "iccid"),
    })
}

/// NL mobile format: `06` + 8 digits.
pub fn validate_phone(phone: &str) -> bool {
    let p = phone.trim();
    p.len() == 10 && p.starts_with("06") && p.bytes().all(|b| b.is_ascii_digit())
}

/// Exactly 6 ASCII digits (OTP / email validation code).
pub fn validate_code(code: &str) -> bool {
    let c = code.trim();
    c.len() == 6 && c.bytes().all(|b| b.is_ascii_digit())
}

/// Best-effort human-readable error from a Simyo error body.
pub fn extract_error_message(root: &Value) -> Option<String> {
    for key in ["message", "error", "reason", "detail", "title"] {
        if let Some(s) = str_field(root, key) {
            return Some(s);
        }
    }
    if let Some(r) = result_obj(root) {
        for key in ["message", "error", "reason"] {
            if let Some(s) = str_field(r, key) {
                return Some(s);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // ---- MFA skip classification ----

    #[test]
    fn mfa_skip_none_or_empty() {
        assert!(is_mfa_skip(None));
        assert!(is_mfa_skip(Some("")));
        assert!(is_mfa_skip(Some("  ")));
    }

    #[test]
    fn mfa_skip_known_statuses_case_insensitive() {
        for s in [
            "DISABLED_BY_CUSTOMER",
            "DISABLED",
            "NONE",
            "OFF",
            "OK",
            "VERIFIED",
            "COMPLETED",
            "NOT_REQUIRED",
            "INACTIVE",
            "DISABLED_BY_OPERATOR",
        ] {
            assert!(is_mfa_skip(Some(s)), "expected skip: {s}");
            assert!(
                is_mfa_skip(Some(&s.to_lowercase())),
                "expected skip (lower): {s}"
            );
        }
    }

    #[test]
    fn mfa_skip_false_for_pending() {
        assert!(!is_mfa_skip(Some("PENDING_VERIFICATION")));
        assert!(!is_mfa_skip(Some("ACTIVE")));
    }

    // ---- MFA pending classification ----

    #[test]
    fn mfa_pending_none_or_empty_is_false() {
        assert!(!is_mfa_pending(None));
        assert!(!is_mfa_pending(Some("")));
        assert!(!is_mfa_pending(Some("  ")));
    }

    #[test]
    fn mfa_pending_exact_statuses() {
        for s in [
            "PENDING_VERIFICATION",
            "PENDING",
            "REQUIRED",
            "CHALLENGE_REQUIRED",
            "ENABLED",
            "ACTIVE",
        ] {
            assert!(is_mfa_pending(Some(s)), "expected pending: {s}");
            assert!(
                is_mfa_pending(Some(&s.to_lowercase())),
                "expected pending (lower): {s}"
            );
        }
    }

    #[test]
    fn mfa_pending_not_required_is_false() {
        assert!(!is_mfa_pending(Some("NOT_REQUIRED")));
        assert!(!is_mfa_pending(Some("not_required")));
    }

    #[test]
    fn mfa_pending_heuristics() {
        assert!(is_mfa_pending(Some("UNKNOWN_PENDING_FOO")));
        assert!(is_mfa_pending(Some("SOME_CHALLENGE")));
        assert!(is_mfa_pending(Some("MFA_REQUIRED_V2")));
        assert!(!is_mfa_pending(Some("SOMETHING_ELSE")));
    }

    // ---- Token extraction ----

    #[test]
    fn extract_session_token_works() {
        assert_eq!(
            extract_session_token(&json!({"result": {"sessionToken": "abc123"}})),
            Some("abc123".to_string())
        );
        assert_eq!(extract_session_token(&json!({"result": {}})), None);
        assert_eq!(extract_session_token(&json!({})), None);
        assert_eq!(
            extract_session_token(&json!({"result": {"sessionToken": null}})),
            None
        );
        assert_eq!(
            extract_session_token(&json!({"result": {"sessionToken": 123}})),
            None
        );
        assert_eq!(
            extract_session_token(&json!({"result": {"sessionToken": ""}})),
            None
        );
    }

    #[test]
    fn extract_formal_token_prefers_token_then_session() {
        assert_eq!(
            extract_formal_token(&json!({"result": {"token": "formal"}})),
            Some("formal".to_string())
        );
        assert_eq!(
            extract_formal_token(&json!({"result": {"token": "formal", "sessionToken": "temp"}})),
            Some("formal".to_string())
        );
        assert_eq!(
            extract_formal_token(&json!({"result": {"sessionToken": "temp"}})),
            Some("temp".to_string())
        );
        assert_eq!(extract_formal_token(&json!({})), None);
    }

    // ---- MFA status / method extraction ----

    #[test]
    fn extract_mfa_fields() {
        let body = json!({"result": {"mfaStatus": "PENDING_VERIFICATION", "mfaMethod": "SMS"}});
        assert_eq!(
            extract_mfa_status(&body),
            Some("PENDING_VERIFICATION".to_string())
        );
        assert_eq!(extract_mfa_method(&body), Some("SMS".to_string()));
        assert_eq!(extract_mfa_status(&json!({"result": {}})), None);
        assert_eq!(extract_mfa_status(&json!({})), None);
    }

    // ---- eSIM info extraction ----

    #[test]
    fn extract_activation_code_works() {
        assert_eq!(
            extract_activation_code(&json!({"result": {"activationCode": "LPA:1$x$y"}})),
            Some("LPA:1$x$y".to_string())
        );
        assert_eq!(extract_activation_code(&json!({"result": {}})), None);
        assert_eq!(extract_activation_code(&json!({})), None);
    }

    #[test]
    fn extract_esim_info_full_and_partial() {
        let full = json!({
            "result": {
                "activationCode": "AC1",
                "status": "READY",
                "phoneNumber": "0612345678",
                "iccid": "8988229000000000000"
            }
        });
        let info = extract_esim_info(&full).expect("full info");
        assert_eq!(info.activation_code, "AC1");
        assert_eq!(info.status, Some("READY".to_string()));
        assert_eq!(info.phone_number, Some("0612345678".to_string()));
        assert_eq!(info.iccid, Some("8988229000000000000".to_string()));

        let partial = json!({"result": {"activationCode": "AC2"}});
        let info2 = extract_esim_info(&partial).expect("partial info");
        assert_eq!(info2.activation_code, "AC2");
        assert_eq!(info2.status, None);
        assert_eq!(info2.phone_number, None);
        assert_eq!(info2.iccid, None);

        assert_eq!(extract_esim_info(&json!({"result": {}})), None);
        assert_eq!(extract_esim_info(&json!({})), None);
    }

    // ---- Input validation ----

    #[test]
    fn validate_phone_nl_format() {
        assert!(validate_phone("0612345678"));
        assert!(!validate_phone(""));
        assert!(!validate_phone("061234567")); // 9 digits
        assert!(!validate_phone("06123456789")); // 11 digits
        assert!(!validate_phone("31612345678")); // +31 form
        assert!(!validate_phone("06 12345678")); // space
        assert!(!validate_phone("061234567x")); // non digit
    }

    #[test]
    fn validate_code_six_digits() {
        assert!(validate_code("123456"));
        assert!(!validate_code("12345"));
        assert!(!validate_code("1234567"));
        assert!(!validate_code("12345a"));
        assert!(!validate_code(""));
    }

    // ---- Error message extraction ----

    #[test]
    fn extract_error_message_top_level() {
        assert_eq!(
            extract_error_message(&json!({"message": "bad"})),
            Some("bad".to_string())
        );
        assert_eq!(
            extract_error_message(&json!({"error": "err"})),
            Some("err".to_string())
        );
        assert_eq!(
            extract_error_message(&json!({"reason": "why"})),
            Some("why".to_string())
        );
    }

    #[test]
    fn extract_error_message_from_result() {
        assert_eq!(
            extract_error_message(&json!({"result": {"reason": "deep"}})),
            Some("deep".to_string())
        );
        assert_eq!(
            extract_error_message(&json!({"result": {"success": false, "message": "m"}})),
            Some("m".to_string())
        );
    }

    #[test]
    fn extract_error_message_none() {
        assert_eq!(extract_error_message(&json!({})), None);
        assert_eq!(extract_error_message(&json!({"result": {}})), None);
    }

    // ---- eSIM status constants ----

    #[test]
    fn esim_status_constants_match_js() {
        assert_eq!(
            ESIM_WAITING_FOR_VALIDATION_CODE,
            "ESIM_REQUEST_WAITING_FOR_VALIDATION_CODE"
        );
        assert_eq!(ESIM_READY_FOR_DOWNLOAD, "ESIM_REQUEST_READY_FOR_DOWNLOAD");
        assert_eq!(ESIM_START_REQUEST, "ESIM_START_REQUEST");
    }
}
