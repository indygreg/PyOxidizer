# x509-certificate

`x509-certificate` is a library crate for interfacing with X.509 certificates.
It supports the following:

* Parsing certificates from BER, DER, and PEM.
* Serializing certificates to BER, DER, and PEM.
* Defining common algorithm identifiers.

**This crate has not undergone a security audit. It does not
employ many protections for malformed data when parsing certificates.
Use at your own risk.**
