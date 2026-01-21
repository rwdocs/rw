//! RSA private key loading for OAuth 1.0 RSA-SHA1.

use std::path::Path;

use rsa::RsaPrivateKey;
use rsa::pkcs1::DecodeRsaPrivateKey;
use rsa::pkcs8::DecodePrivateKey;

use crate::error::ConfluenceError;

/// Read RSA private key from PEM file.
pub fn read_private_key(path: impl AsRef<Path>) -> Result<Vec<u8>, ConfluenceError> {
    let path = path.as_ref();
    let data = std::fs::read(path).map_err(|e| {
        ConfluenceError::RsaKey(format!("Failed to read key file {}: {e}", path.display()))
    })?;

    // Validate the key can be parsed
    load_private_key(&data)?;

    Ok(data)
}

/// Load RSA private key from PEM bytes (auto-detects format).
///
/// Supports both PKCS#8 (`-----BEGIN PRIVATE KEY-----`) and
/// PKCS#1 (`-----BEGIN RSA PRIVATE KEY-----`) formats.
pub fn load_private_key(pem: &[u8]) -> Result<RsaPrivateKey, ConfluenceError> {
    let pem_str = std::str::from_utf8(pem)
        .map_err(|e| ConfluenceError::RsaKey(format!("Invalid UTF-8 in key: {e}")))?;

    // Try PKCS#8 first (-----BEGIN PRIVATE KEY-----)
    if let Ok(key) = RsaPrivateKey::from_pkcs8_pem(pem_str) {
        return Ok(key);
    }

    // Fall back to PKCS#1 (-----BEGIN RSA PRIVATE KEY-----)
    RsaPrivateKey::from_pkcs1_pem(pem_str)
        .map_err(|e| ConfluenceError::RsaKey(format!("Failed to parse PEM key: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_PKCS8_KEY: &str = r"-----BEGIN PRIVATE KEY-----
MIIEvQIBADANBgkqhkiG9w0BAQEFAASCBKcwggSjAgEAAoIBAQDXyzisgwj5oXOk
9bXXMCiqDbT70Tkwonl8c7P0Eec1cfCSjqw2cT9oi8zuXlZSmgsh9zPwab/0Uc5j
PFnW5wD5MIFARtSk2BKt8goiej3U7CMp0QL3hXb+ejMaP7kGZ9uYRjnQToou2J2/
02UBRSXrvMNwkvhBlIXtz0Fh6IveWvMEtEQcgn0wn+mc4cEf+zun2kFZ1mia8twI
BduiZPEUetskIMTxfhocwuZYwRJaVbPYh/QM9m2KjfvOWxRcakaKD5+fi8Jb5Oqm
tz27ZYv6M21HnGuOTlRAeIbgP4rv6p7JX3F4sBECl2oonjUQtUg/cjDOWp6JXNch
u+7hr6H5AgMBAAECggEAAl59S0uO/CqdGekGq4ugTqmi3IbiAVovSkH87keKCcir
8vf1BQ3+O7gZMl6/xN1jFObhX5jRni2NvgIqHFVh6dpx+NIuQHcM0XMQUGuWJTHI
ewuL5ErHUSjnSbj8X4khXI0c0mAiXTxMkxAPklF/hpSGcsRyTEoEpGU7mwcSDgld
a2PcPiI1PgfgBggHuD0y9EhFAM4Bs29plLudCWmtEOppgSCGwdNmhA0mQY58xVEA
JMUq4h5ANztz+GqGakMebGvIpssdu+JXLg9RtPthH3PNUg8UNQXBFtE62YOUIIIn
oyGWQSoApfqjUYNSsWSxl66+NdeB2kw9r9o71XihAQKBgQDttragQmkqQzRZ4CLx
jhG+zb92zGIjTRiHe1bVVu/cOWPaFhTmjsc+tWcWFLzvPTOkcJ3/hZzxSFuAgcg7
dZVsivgyTCfcTHixranllKfJhZ3/F+ZOcoSkiqBzr1EFLFP87XdTf2kQhFgpBNGo
E81fMgbfsQRmd+Fimo8N0uCOQQKBgQDoZNcqhoC6jxc3iBFEiIMgLAmccx8N0dC3
xEwxg/RJ1njg1z3mcZoX6Ec+2NU7jlwR+mTUlS2aVHYDFZqOnVicQCEvkQbYt7De
omodKKrdYN0HDZcQcQQtGvTV6ASIOUJBVbB5gOyx3gi196ERzZ/diGhUpHbiNhi5
ssoT3V2VuQKBgEhhUPw9HG5s5hzTnXA1lPunBDx1ARDEocpm6Mqu3PwOUXQPMy/8
m3hhndDgYaLq3LWeQM2T7nSdVpcrbT+Fjwjsy6PtAloWws0/FrM771byI2iP62VJ
g0/ikfaHlEDh/XTPDX1UFzabRYi/2eK2nNr2jZdA/BkDOZJfg11vL0bBAoGAWod9
8kj3OLWpO66721C6k/vTuqh1/nIvtoa3j8pxjZoI+L2glXbHqmyH5Imfd1Xbs/0w
7kc2vpoMZuMxlEDjVer9goQigKX+NpxabgV7mkWzlJ3MrVD5aYDIw9NggJidoMn6
tzpr+lYeWpSeoErT7f7HdcGjtjeQpjZp1hcz77ECgYEA4QxMNusdXfNwxeemDxs2
9S1pQ8Vrzvw8ACcJBZTluKvGuO3hoPMSu8ywt1Sew74a9QbkkfbPmqujc62FHo1+
o6Ypn8ZrOCbdrwdSpQu37/7pcDFMq/HAyf2I43wreDAcYktu33ZiEDTkyYM0ygv/
PmtLs+m8nwD5m6Eay2zt00Q=
-----END PRIVATE KEY-----";

    const TEST_PKCS1_KEY: &str = r"-----BEGIN RSA PRIVATE KEY-----
MIIEowIBAAKCAQEAvkWa4f4eCIBjIOQI92E8Y/RpOoMZHCsr2sgrMJv3gixTp9PI
ourmwIxy06LIRFVwUj0ATZcrfvIe3TdDBSFdBiaqFN163DLEHetRnpvIis+/JhSr
NBhRI+w6IydIpEuVPtjFvaWSDesGz/vGciP8NTpw2JrsaaLhU1CI21R1HOESnCeu
jiX1P+KES5HC+TbYkc3YuT2pcArVPV0n+TUoumMoODG3OoSZpczWiil/wcSm5yVD
wMS3nJ69Yip0bEOC/oDuhYXxCdRzLNfM+d9enUfUeTa2TjXRItCf8L89RFh/mG7k
5vM6Lw/pAqlVTQ0bsf0wBFHyd9EmhylNd6lBpwIDAQABAoIBAAdZItElgj1rz+g+
RkZff/iQQNwcogSfejMZ1ekrrNRfJ9/sYuw/zCeVhP29ZKSW9B6I8pEMjIZ7jUuX
zcPN9Qy+w4TIxOzqHP7WzrEkbQtHWKd81tvNMJ0Fi30+ECUgMiRaNNDNKyXUdM/l
dlWEJEJd/muvGABAZRlVm+N0kI1L5tmls5csj3O5td5sR1cufJqDTTjaQ91/Pbq6
ZjP1FUULQNCLOPpkFpV9WGvinXkLXhWyXE/McJ+LN1ehMusMXeyXKU5bevwrKx0V
AOqmZQcygtr/oo+jP8uYo5Hu5Yi3saoADbIJuR9FSK9q5XC5dn72bHw/zWD5PiM6
A/jG2xkCgYEA66rYzUkjLOn5aDMqsU7/g0TUFjUol+0ebfhfwDZRLuNmjgxQFurx
nZehNmhbc46QHfDE+IvsTX9YJYfEQF0Us8OEWTEx63YcobHsDB0hqLorwRqDqqkn
BHH73SbF8bb3jyQpru2tEBkfmJakktdvsjwUZK9t1UaLLsT0xIECm/UCgYEAzrAd
S2JT8HYo32T0fqdM5iQ5NH3c9gYZSgqdoJG0ahlJFS9q7e3CbKxenKt16dhKklzK
ssN8bilt8030Qf4PdQM/JdB0qEFEyXbc/Hjg8RlE7ufnla5h5cpghw4ZT6MaGT8y
h5bRH11oYPBcEBOdSdwtBbnOM/kTFImmM+u+oasCgYAO4UizHY0VBujyhViKvXww
o6XoiQ65GQW019vj3QofNciB65EbAVakJrDNKKWtlDDRUyR8tQkEk6fTJtFjZv0p
pIy0vQBz549DPmKzGRvI9YhRtwTdP7Tw+Ol4OzyAWJck+JmRK/yjaZKvJcPaabPp
wxVejh0XPE8JcvIpfiPWCQKBgQCCag2R61EbgPL5QjIjWFzlOiX+QQyTD/YVCe27
yzQTXjEG9Qx7ZHIxL/Hi2S1lh9xFzdb4RPDWcb22r5FXsn7+TjYiHg39vHEyzZVS
mNMWTeN4+0rc31NWMwQFM5g0760gEQhJFZiOOdoKkJ8GZdnxKMQfwMWjdZ3cb543
VKDMUQKBgBHhfMW5MMSmqN7JMerV0veOBfxFuO79RXU7RTyzeTvv2PCk0fSogBUh
Hr3IEdDPzGyYHhZBgkf+ngZgtYaC0E/LeoE6CD8CrYRC/SOKZ31suiZNm8qjLhOb
WllcHcctfHBEqp2XEP7wnipa9TmZWKeCON72FK21eRUJ2drlPyc8
-----END RSA PRIVATE KEY-----";

    #[test]
    fn test_load_pkcs8_key() {
        let result = load_private_key(TEST_PKCS8_KEY.as_bytes());
        result.unwrap();
    }

    #[test]
    fn test_load_pkcs1_key() {
        let result = load_private_key(TEST_PKCS1_KEY.as_bytes());
        assert!(result.is_ok());
    }

    #[test]
    fn test_invalid_key() {
        let result = load_private_key(b"not a valid key");
        assert!(result.is_err());
    }
}
