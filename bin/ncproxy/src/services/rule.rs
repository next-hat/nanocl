use ntex::web;

use nanocl_error::http::HttpError;
use nanocld_client::stubs::proxy::ResourceProxyRule;

use crate::{models::SystemStateRef, utils};

/// Create/Update a new ProxyRule
#[cfg_attr(feature = "dev", utoipa::path(
  put,
  tag = "Rules",
  path = "/rules/{name}",
  request_body = ResourceProxyRule,
  params(
    ("name" = String, Path, description = "Name of the rule"),
  ),
  responses(
    (status = 200, description = "The created rule", body = ResourceProxyRule),
  ),
))]
pub async fn apply_rule(
  state: &web::AppState<SystemStateRef>,
  _request_state: (),
  path: web::types::Path<(String, String)>,
  payload: web::types::Json<ResourceProxyRule>,
) -> Result<web::HttpResponse, HttpError> {
  log::info!("apply_rule: {}", path.1);
  utils::nginx::add_rule(&path.1, &payload, &state).await?;
  state.event_emitter.emit_reload().await;
  Ok(web::HttpResponse::Ok().json(&payload.into_inner()))
}

/// Delete a ProxyRule
#[cfg_attr(feature = "dev", utoipa::path(
  delete,
  tag = "Rules",
  path = "/rules/{name}",
  params(
    ("name" = String, Path, description = "Name of the rule"),
  ),
  responses(
    (status = 200, description = "Rule has been deleted"),
  ),
))]
pub async fn remove_rule(
  state: &web::AppState<SystemStateRef>,
  _request_state: (),
  path: web::types::Path<(String, String)>,
) -> Result<web::HttpResponse, HttpError> {
  log::info!("remove_rule: {}", path.1);
  utils::nginx::del_rule(&path.1, &state).await?;
  state.event_emitter.emit_reload().await;
  Ok(web::HttpResponse::Ok().build())
}

pub fn ntex_config(
  config: &mut web::ServiceConfig<web::AppState<crate::models::SystemStateRef>>,
) {
  config.service(
    web::resource("/rules/{name}")
      .name("apply_rule")
      .guard(web::guard::Put())
      .to_with_state(apply_rule),
  );
  config.service(
    web::resource("/rules/{name}")
      .name("remove_rule")
      .guard(web::guard::Delete())
      .to_with_state(remove_rule),
  );
}

#[cfg(test)]
mod tests {
  use ntex::http;

  use crate::utils::tests::*;

  #[ntex::test]
  async fn basic() {
    let name = "ncproxy-io-test-resource";
    let client = gen_default_test_client().await;
    ensure_test_cargo().await.unwrap();
    let payload = read_rule("tests/basic.yml").unwrap();
    let res = client
      .send_put(&format!("/rules/{name}"), Some(&payload), None::<String>)
      .await;
    let json = res.json::<serde_yaml::Value>().await.unwrap();
    println!("{:?}", json);
    test_status_code!(res.status(), http::StatusCode::OK, "put a rule");
    let res = client
      .send_delete(&format!("/rules/{name}"), None::<String>)
      .await;
    test_status_code!(res.status(), http::StatusCode::OK, "delete a rule");
    clean_test_cargo().await.unwrap();
  }

  #[ntex::test]
  async fn http3_template() {
    use nanocld_client::{
      ConnectOpts, NanocldClient,
      stubs::{generic::NetworkKind, proxy::*, secret::SecretPartial},
    };
    use openssl::{
      asn1::Asn1Time,
      bn::BigNum,
      hash::MessageDigest,
      pkey::PKey,
      rsa::Rsa,
      x509::{X509, X509NameBuilder},
    };

    let name = "ncproxy-io-test-http3";
    let client = gen_default_test_client().await;
    ensure_test_cargo().await.unwrap();
    let nanocld = NanocldClient::connect_to(&ConnectOpts {
      url: "http://nanocl.internal:8585".into(),
      ..Default::default()
    })
    .unwrap();
    // Own the TLS fixture instead of depending on an externally created secret.
    let secret_name = format!(
      "{name}-{}",
      std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos()
    );
    let key = PKey::from_rsa(Rsa::generate(2048).unwrap()).unwrap();
    let mut subject = X509NameBuilder::new().unwrap();
    subject.append_entry_by_text("CN", "h3.test").unwrap();
    let subject = subject.build();
    let mut certificate = X509::builder().unwrap();
    certificate.set_version(2).unwrap();
    certificate
      .set_serial_number(
        &BigNum::from_u32(1).unwrap().to_asn1_integer().unwrap(),
      )
      .unwrap();
    certificate.set_subject_name(&subject).unwrap();
    certificate.set_issuer_name(&subject).unwrap();
    certificate.set_pubkey(&key).unwrap();
    certificate
      .set_not_before(&Asn1Time::days_from_now(0).unwrap())
      .unwrap();
    certificate
      .set_not_after(&Asn1Time::days_from_now(1).unwrap())
      .unwrap();
    certificate.sign(&key, MessageDigest::sha256()).unwrap();
    nanocld
      .create_secret(&SecretPartial {
        name: secret_name.clone(),
        kind: "nanocl.io/tls".into(),
        immutable: false,
        metadata: None,
        data: serde_json::json!({
          "Certificate": String::from_utf8(certificate.build().to_pem().unwrap()).unwrap(),
          "CertificateKey": String::from_utf8(key.private_key_to_pem_pkcs8().unwrap()).unwrap(),
        }),
      })
      .await
      .unwrap();
    let rule = ResourceProxyRule {
      rules: vec![ProxyRule::Http(ProxyRuleHttp {
        domain: Some("h3.test".into()),
        port: Some(443),
        hsts: None,
        network: NetworkKind::All,
        limit_req_zone: None,
        locations: vec![ProxyHttpLocation {
          path: "/".into(),
          target: LocationTarget::Upstream(UpstreamTarget {
            key: "global.ncproxy-test.c".into(),
            port: 9000,
            path: None,
            enable_logging: None,
            ssl: None,
          }),
          limit_req: None,
          allowed_ips: None,
          headers: None,
          version: None,
          client_max_body_size: None,
          client_body_timeout: None,
          request_buffering: None,
          response_buffering: None,
          cache: None,
          read_timeout: None,
          send_timeout: None,
          connect_timeout: None,
          next_upstream: None,
          intercept_errors: None,
        }],
        ssl: Some(ProxySsl::Secret(secret_name.clone())),
        http3: Some(ProxyHttp3::Bool(true)),
        includes: None,
      })],
    };
    let res = client
      .send_put(&format!("/rules/{name}"), Some(&rule), None::<String>)
      .await;
    let status = res.status();
    let body = res.body().await;
    let home = std::env::var("HOME").unwrap();
    let rendered = std::fs::read_to_string(format!(
      "{home}/.nanocl_dev/state/proxy/sites-available/{name}.conf"
    ));
    let res = client
      .send_delete(&format!("/rules/{name}"), None::<String>)
      .await;
    let secret_cleanup = nanocld.delete_secret(&secret_name).await;
    let cargo_cleanup = clean_test_cargo().await;
    let file_cleanup = ["cert", "key"].map(|extension| {
      match std::fs::remove_file(format!(
        "{home}/.nanocl_dev/state/proxy/secrets/{secret_name}.{extension}"
      )) {
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        result => result,
      }
    });
    // Clean up before assertions so a rejected rule does not leak its secret.
    let body = body.unwrap();
    assert_eq!(
      status,
      http::StatusCode::OK,
      "put h3 rule: {}",
      String::from_utf8_lossy(&body),
    );
    test_status_code!(res.status(), http::StatusCode::OK, "delete h3 rule");
    secret_cleanup.unwrap();
    cargo_cleanup.unwrap();
    for cleanup in file_cleanup {
      cleanup.unwrap();
    }
    let rendered = rendered.unwrap();
    assert!(rendered.lines().any(|line| {
      let line = line.trim();
      line.starts_with("listen ") && line.ends_with(" quic;")
    }));
    assert!(rendered.lines().any(|line| line.trim() == "http3 on;"));
  }
}
