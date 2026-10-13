//! Prints a code-signing identity, a self-signed certificate and its PKCS#8 key in PEM, derived
//! from the 64-hex-character seed file named by the first argument. Equal seeds give byte-equal
//! identities, so code signed by any of them meets one designated requirement.
//!
//! ```sh
//! signing-identity seed.key | rcodesign sign --pem-file /dev/stdin App.app
//! ```

use std::{env, fs, io::Write, str::FromStr};

use anyhow::{Context, bail};
use der::{Any, DateTime, EncodePem, asn1::ObjectIdentifier, pem::LineEnding};
use p256::{
    SecretKey,
    ecdsa::{
        DerSignature, SigningKey, VerifyingKey,
        signature::{Error, Keypair, Signer},
    },
    pkcs8::EncodePrivateKey,
};
use spki::{AlgorithmIdentifierOwned, DynSignatureAlgorithmIdentifier, SubjectPublicKeyInfo};
use x509_cert::{
    builder::{Builder, CertificateBuilder, profile::BuilderProfile},
    certificate::TbsCertificate,
    ext::{
        Extension,
        pkix::{BasicConstraints, ExtendedKeyUsage, KeyUsage, KeyUsages},
    },
    name::Name,
    serial_number::SerialNumber,
    time::{Time, Validity},
};
use zeroize::Zeroizing;

/// The BLAKE3 `derive_key` context of the signing key.
const CONTEXT: &str = "something-in-reality 2026-10-13 code signing key";
const SUBJECT: &str = "CN=something-in-reality";
const ECDSA_WITH_SHA256: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.2.840.10045.4.3.2");
const CODE_SIGNING: ObjectIdentifier = ObjectIdentifier::new_unwrap("1.3.6.1.5.5.7.3.3");

/// A certificate whose issuer is its subject.
struct SelfSigned(Name);

impl BuilderProfile for SelfSigned {
    fn get_issuer(&self, _subject: &Name) -> Name {
        self.0.clone()
    }

    fn get_subject(&self) -> Name {
        self.0.clone()
    }

    fn build_extensions(
        &self,
        _spk: spki::SubjectPublicKeyInfoRef<'_>,
        _issuer_spk: spki::SubjectPublicKeyInfoRef<'_>,
        _tbs: &TbsCertificate,
    ) -> x509_cert::builder::Result<Vec<Extension>> {
        Ok(Vec::new())
    }
}

/// Deterministic ECDSA (RFC 6979) with NULL algorithm parameters: rcodesign embeds certificates
/// in that encoding, so the embedded certificate is byte-equal to this one.
struct NullParameters(SigningKey);

impl Keypair for NullParameters {
    type VerifyingKey = VerifyingKey;

    fn verifying_key(&self) -> VerifyingKey {
        *self.0.verifying_key()
    }
}

impl Signer<DerSignature> for NullParameters {
    fn try_sign(&self, message: &[u8]) -> Result<DerSignature, Error> {
        self.0.try_sign(message)
    }
}

impl DynSignatureAlgorithmIdentifier for NullParameters {
    fn signature_algorithm_identifier(&self) -> spki::Result<AlgorithmIdentifierOwned> {
        Ok(AlgorithmIdentifierOwned {
            oid: ECDSA_WITH_SHA256,
            parameters: Some(Any::null()),
        })
    }
}

fn main() -> anyhow::Result<()> {
    let Some(path) = env::args_os().nth(1) else {
        bail!("usage: signing-identity <seed-file>");
    };
    let text = Zeroizing::new(fs::read(&path).context("reading the seed")?);
    let mut seed = Zeroizing::new([0; 32]);
    hex::decode_to_slice(text.trim_ascii(), seed.as_mut_slice())
        .context("the seed is 64 hex characters")?;
    let key = Zeroizing::new(blake3::derive_key(CONTEXT, seed.as_slice()));
    let secret = SecretKey::from_slice(key.as_slice())?;
    let signer = SigningKey::from(&secret);

    let validity = Validity::new(
        Time::from(DateTime::new(2026, 1, 1, 0, 0, 0)?),
        Time::from(DateTime::new(2126, 1, 1, 0, 0, 0)?),
    );
    let mut builder = CertificateBuilder::new(
        SelfSigned(Name::from_str(SUBJECT)?),
        SerialNumber::from(1u32),
        validity,
        SubjectPublicKeyInfo::from_key(signer.verifying_key())?,
    )?;
    builder.add_extension((
        true,
        &BasicConstraints {
            ca: false,
            path_len_constraint: None,
        },
    ))?;
    builder.add_extension((true, &KeyUsage(KeyUsages::DigitalSignature.into())))?;
    builder.add_extension(&ExtendedKeyUsage(vec![CODE_SIGNING]))?;
    let certificate = builder.build::<_, DerSignature>(&NullParameters(signer))?;

    let mut stdout = std::io::stdout().lock();
    stdout.write_all(certificate.to_pem(LineEnding::LF)?.as_bytes())?;
    stdout.write_all(secret.to_pkcs8_pem(LineEnding::LF)?.as_bytes())?;
    Ok(())
}
