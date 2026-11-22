//! TLS for the link. The desktop's certificate is self-signed, so no
//! authority vouches for it: the phone trusts exactly the certificate whose
//! fingerprint it scanned, and nothing else.

use rustls::crypto::ring::default_provider;
use rustls::crypto::CryptoProvider;
use std::sync::Arc;

fn provider() -> Arc<CryptoProvider> {
    // Passed explicitly everywhere, so the link never depends on (or sets)
    // the process-wide default another library might also want.
    Arc::new(default_provider())
}

#[cfg(feature = "server")]
pub use server::Identity;

#[cfg(feature = "server")]
mod server {
    use super::provider;
    use crate::pairing::{fingerprint, from_hex, to_hex};
    use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
    use rustls::ServerConfig;
    use serde::{Deserialize, Serialize};
    use std::sync::Arc;

    /// The desktop's certificate and private key. Made once and kept: paired
    /// phones recognise the desktop by it, so a new one unpairs them all.
    #[derive(Clone, Serialize, Deserialize)]
    pub struct Identity {
        /// DER, in hex.
        cert: String,
        /// PKCS#8 DER, in hex.
        key: String,
    }

    impl std::fmt::Debug for Identity {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            // Never the key.
            write!(f, "Identity({})", self.fingerprint())
        }
    }

    impl Identity {
        pub fn generate() -> Result<Self, String> {
            let certified = rcgen::generate_simple_self_signed(vec!["handy.local".to_string()])
                .map_err(|e| format!("Failed to generate a certificate: {e}"))?;
            Ok(Self {
                cert: to_hex(certified.cert.der()),
                key: to_hex(&certified.signing_key.serialize_der()),
            })
        }

        fn cert_der(&self) -> Vec<u8> {
            from_hex(&self.cert).unwrap_or_default()
        }

        /// Whether both halves decode, which a hand-edited file may not.
        pub fn is_valid(&self) -> bool {
            self.server_config().is_ok()
        }

        pub fn fingerprint(&self) -> String {
            fingerprint(&self.cert_der())
        }

        pub(crate) fn server_config(&self) -> Result<Arc<ServerConfig>, String> {
            let cert = from_hex(&self.cert).ok_or("The stored certificate is damaged")?;
            let key = from_hex(&self.key).ok_or("The stored private key is damaged")?;
            let config = ServerConfig::builder_with_provider(provider())
                .with_safe_default_protocol_versions()
                .map_err(|e| e.to_string())?
                .with_no_client_auth()
                .with_single_cert(
                    vec![CertificateDer::from(cert)],
                    PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(key)),
                )
                .map_err(|e| format!("The stored certificate can't be used: {e}"))?;
            Ok(Arc::new(config))
        }
    }
}

#[cfg(feature = "client")]
pub(crate) use client::pinned_client_config;

#[cfg(feature = "client")]
mod client {
    use super::provider;
    use crate::pairing::{constant_time_eq, fingerprint};
    use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
    use rustls::crypto::{verify_tls12_signature, verify_tls13_signature, CryptoProvider};
    use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
    use rustls::{ClientConfig, DigitallySignedStruct, Error, SignatureScheme};
    use std::sync::Arc;

    /// Accepts the one certificate with the pinned fingerprint. Its name and
    /// dates don't matter; holding the matching private key does, and the
    /// handshake signature checks below prove that.
    #[derive(Debug)]
    struct PinnedCert {
        fingerprint: String,
        provider: Arc<CryptoProvider>,
    }

    impl ServerCertVerifier for PinnedCert {
        fn verify_server_cert(
            &self,
            end_entity: &CertificateDer<'_>,
            _intermediates: &[CertificateDer<'_>],
            _server_name: &ServerName<'_>,
            _ocsp_response: &[u8],
            _now: UnixTime,
        ) -> Result<ServerCertVerified, Error> {
            if constant_time_eq(
                fingerprint(end_entity).as_bytes(),
                self.fingerprint.as_bytes(),
            ) {
                Ok(ServerCertVerified::assertion())
            } else {
                Err(Error::General(
                    "this is not the desktop this phone was paired with".into(),
                ))
            }
        }

        fn verify_tls12_signature(
            &self,
            message: &[u8],
            cert: &CertificateDer<'_>,
            dss: &DigitallySignedStruct,
        ) -> Result<HandshakeSignatureValid, Error> {
            verify_tls12_signature(
                message,
                cert,
                dss,
                &self.provider.signature_verification_algorithms,
            )
        }

        fn verify_tls13_signature(
            &self,
            message: &[u8],
            cert: &CertificateDer<'_>,
            dss: &DigitallySignedStruct,
        ) -> Result<HandshakeSignatureValid, Error> {
            verify_tls13_signature(
                message,
                cert,
                dss,
                &self.provider.signature_verification_algorithms,
            )
        }

        fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
            self.provider
                .signature_verification_algorithms
                .supported_schemes()
        }
    }

    pub(crate) fn pinned_client_config(fingerprint: &str) -> Result<Arc<ClientConfig>, String> {
        let provider = provider();
        let config = ClientConfig::builder_with_provider(Arc::clone(&provider))
            .with_safe_default_protocol_versions()
            .map_err(|e| e.to_string())?
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(PinnedCert {
                fingerprint: fingerprint.to_ascii_lowercase(),
                provider,
            }))
            .with_no_client_auth();
        Ok(Arc::new(config))
    }
}
