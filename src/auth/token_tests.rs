//! Generate test-only signing keys in memory; no account credentials are used.
use super::*;
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use jsonwebtoken::{EncodingKey, Header, encode};
use ring::{
    rand::SystemRandom,
    signature::{ECDSA_P256_SHA256_FIXED_SIGNING, EcdsaKeyPair, KeyPair},
};
use serde_json::{Value, json};
use std::sync::OnceLock;

fn signing_key() -> &'static (EncodingKey, JwkSet) {
    static KEY: OnceLock<(EncodingKey, JwkSet)> = OnceLock::new();
    KEY.get_or_init(|| {
        let algorithm = &ECDSA_P256_SHA256_FIXED_SIGNING;
        let rng = SystemRandom::new();
        let der = EcdsaKeyPair::generate_pkcs8(algorithm, &rng).unwrap();
        let pair = EcdsaKeyPair::from_pkcs8(algorithm, der.as_ref(), &rng).unwrap();
        // An uncompressed P-256 point is a tag followed by two 32-byte coordinates.
        let point = pair.public_key().as_ref();
        let jwks = serde_json::from_value(json!({"keys":[{
            "kty":"EC", "crv":"P-256", "kid":"test", "alg":"ES256",
            "x":URL_SAFE_NO_PAD.encode(&point[1..33]), "y":URL_SAFE_NO_PAD.encode(&point[33..])
        }]}))
        .unwrap();
        (EncodingKey::from_ec_der(der.as_ref()), jwks)
    })
}

fn keys() -> JwkSet {
    signing_key().1.clone()
}

fn claims() -> Value {
    json!({"iss":ISSUER,"aud":"client","sub":"user","iat":now(),"exp":now()+60,"nonce":"expected"})
}

fn sign(claims: &Value) -> String {
    let mut header = Header::new(Algorithm::ES256);
    header.kid = Some("test".into());
    encode(&header, claims, &signing_key().0).unwrap()
}

#[test]
fn a_signed_token_requires_the_expected_issuer_audience_expiry_and_nonce() {
    assert_eq!(
        validate(&keys(), &sign(&claims()), "client", Some("expected"))
            .unwrap()
            .sub,
        "user"
    );
    for (field, value) in [
        ("iss", json!("https://attacker.example")),
        ("aud", json!("another-client")),
        ("exp", json!(now() - 60)),
        ("nonce", json!("wrong")),
        ("sub", json!("")),
    ] {
        let mut invalid = claims();
        invalid[field] = value;
        assert!(
            validate(&keys(), &sign(&invalid), "client", Some("expected")).is_err(),
            "Accepted wrong {field}"
        );
    }
    let mut missing = claims();
    missing.as_object_mut().unwrap().remove("nonce");
    assert!(validate(&keys(), &sign(&missing), "client", Some("expected")).is_err());
}

#[test]
fn signature_tampering_and_unexpected_algorithms_are_rejected() {
    let mut token = sign(&claims()).into_bytes();
    let signature = token.iter().rposition(|b| *b == b'.').unwrap() + 1;
    token[signature] = if token[signature] == b'A' { b'B' } else { b'A' };
    assert!(
        validate(
            &keys(),
            std::str::from_utf8(&token).unwrap(),
            "client",
            Some("expected")
        )
        .is_err()
    );
    let token = encode(
        &Header::new(Algorithm::HS256),
        &claims(),
        &EncodingKey::from_secret(b"attacker"),
    )
    .unwrap();
    assert!(validate(&keys(), &token, "client", Some("expected")).is_err());
}

#[test]
fn identity_scopes_do_not_imply_plan_permission() {
    assert!(check_scope("openid email profile").is_err());
    assert!(check_scope("chatgpt.tokens.use.direct.extra").is_err());
    assert!(check_scope("openid chatgpt.tokens.use.direct offline_access").is_ok());
}
