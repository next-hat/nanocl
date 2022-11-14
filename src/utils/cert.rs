use crate::cli::errors::CliError;

use super::exec::_exec;

pub struct _CaCertOptions {
  organization_name: String,
  common_name: String,
}

pub async fn _gen_ca_cert(
  options: _CaCertOptions,
  directory: String,
) -> Result<(), CliError> {
  let ca_cnf = format!("# OpenSSL CA configuration file gen by nanocl\
  [ ca ]\
  default_ca = CA_default\
  \
  [ CA_default ]\
  default_days = 365\
  database = index.txt\
  serial = serial.txt\
  default_md = sha256\
  copy_extensions = copy\
  unique_subject = no\
  \
  # Used to create the CA certificate.\
  [ req ]\
  prompt=no\
  distinguished_name = distinguished_name\
  x509_extensions = extensions\
  \
  [ distinguished_name ]\
  organizationName = {organization_name}\
  commonName = {common_name}\
  \
  [ extensions ]\
  keyUsage = critical,digitalSignature,nonRepudiation,keyEncipherment,keyCertSign\
  basicConstraints = critical,CA:true,pathlen:1\
  \
  # Common policy for nodes and users.\
  [ signing_policy ]\
  organizationName = supplied\
  commonName = optional\
  \
  # Used to sign node certificates.\
  [ signing_node_req ]\
  keyUsage = critical,digitalSignature,keyEncipherment\
  extendedKeyUsage = serverAuth,clientAuth\
  \
  # Used to sign client certificates.\
  [ signing_client_req ]\
  keyUsage = critical,digitalSignature,keyEncipherment\
  extendedKeyUsage = clientAuth\
  ",
    organization_name = options.organization_name,
    common_name = options.common_name,
  );

  println!("{ca_cnf}");

  let output = _exec(
    "openssl",
    &vec!["genrsa", "--out", "my-safe-directory/ca.key", "2048"],
    Some(&directory),
  )
  .await?;

  if !output.status.success() {
    eprintln!("{:?}", &output.stderr);
  }

  Ok(())
}
