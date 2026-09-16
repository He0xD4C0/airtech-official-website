use super::*;

pub(super) fn parse_csv(csv: &str) -> Result<Vec<(i32, Vec<String>)>, ApiError> {
    let mut records = Vec::new();
    let mut record = Vec::new();
    let mut field = String::new();
    let mut quoted = false;
    let mut chars = csv.trim_start_matches('\u{feff}').chars().peekable();
    let mut physical_line = 1_i32;
    let mut record_line = 1_i32;
    while let Some(character) = chars.next() {
        match character {
            '"' if quoted && chars.peek() == Some(&'"') => {
                chars.next();
                field.push('"');
            }
            '"' => quoted = !quoted,
            ',' if !quoted => record.push(std::mem::take(&mut field)),
            '\r' if !quoted => {
                if chars.peek() == Some(&'\n') {
                    chars.next();
                }
                record.push(std::mem::take(&mut field));
                records.push((record_line, std::mem::take(&mut record)));
                physical_line += 1;
                record_line = physical_line;
            }
            '\n' if !quoted => {
                record.push(std::mem::take(&mut field));
                records.push((record_line, std::mem::take(&mut record)));
                physical_line += 1;
                record_line = physical_line;
            }
            '\n' => {
                field.push('\n');
                physical_line += 1;
            }
            value => field.push(value),
        }
    }
    if quoted {
        return Err(ApiError::bad_request(format!(
            "csv contains an unterminated quoted field beginning near row {record_line}."
        )));
    }
    if !field.is_empty() || !record.is_empty() {
        record.push(field);
        records.push((record_line, record));
    }
    Ok(records)
}

pub(super) fn encrypt_confidential(
    key: &ProductStagingEncryptionKey,
    aad: &[u8],
    plaintext: &[u8],
) -> Result<Vec<u8>, ApiError> {
    let unbound = UnboundKey::new(&aead::AES_256_GCM, key.as_bytes())
        .map_err(|_| ApiError::internal("Product staging encryption setup failed."))?;
    let key = LessSafeKey::new(unbound);
    let mut nonce_bytes = [0_u8; 12];
    SystemRandom::new()
        .fill(&mut nonce_bytes)
        .map_err(|_| ApiError::internal("Secure random generation failed."))?;
    let nonce = Nonce::assume_unique_for_key(nonce_bytes);
    let mut ciphertext = plaintext.to_vec();
    key.seal_in_place_append_tag(nonce, Aad::from(aad), &mut ciphertext)
        .map_err(|_| ApiError::internal("Product staging encryption failed."))?;
    let mut nonce_prefixed = nonce_bytes.to_vec();
    nonce_prefixed.extend(ciphertext);
    Ok(nonce_prefixed)
}

/// Decrypt a private source row and return only explicitly allow-listed
/// pricing fields. Callers must separately enforce `product.pricing.read`,
/// emit an audit record, and return `Cache-Control: no-store`.
///
/// The authenticated-data tuple binds ciphertext to the exact import mapping,
/// file checksum, physical row, and stable product id. Moving ciphertext
/// between rows or import runs therefore fails closed.
pub fn decrypt_private_pricing(
    key: &ProductStagingEncryptionKey,
    envelope: PrivatePricingEnvelope<'_>,
) -> Result<BTreeMap<String, String>, ApiError> {
    let nonce: [u8; 12] = envelope.nonce.try_into().map_err(|_| {
        ApiError::service_unavailable("Private Product Master staging metadata is invalid.")
    })?;
    if envelope.authentication_tag.len() != 16 {
        return Err(ApiError::service_unavailable(
            "Private Product Master staging metadata is invalid.",
        ));
    }
    let unbound = UnboundKey::new(&aead::AES_256_GCM, key.as_bytes())
        .map_err(|_| ApiError::internal("Product staging decryption setup failed."))?;
    let key = LessSafeKey::new(unbound);
    let mut ciphertext_and_tag =
        Vec::with_capacity(envelope.ciphertext.len() + envelope.authentication_tag.len());
    ciphertext_and_tag.extend_from_slice(envelope.ciphertext);
    ciphertext_and_tag.extend_from_slice(envelope.authentication_tag);
    let aad = format!(
        "{}:{}:{}:{}",
        envelope.mapping_version, envelope.checksum, envelope.source_row_number, envelope.stable_id
    );
    let plaintext = key
        .open_in_place(
            Nonce::assume_unique_for_key(nonce),
            Aad::from(aad.as_bytes()),
            &mut ciphertext_and_tag,
        )
        .map_err(|_| {
            ApiError::service_unavailable("Private Product Master staging authentication failed.")
        })?;
    let source = serde_json::from_slice::<Map<String, Value>>(plaintext)
        .map_err(|_| ApiError::service_unavailable("Private Product Master staging is invalid."))?;
    Ok(source
        .into_iter()
        .filter_map(|(name, value)| {
            (is_private_pricing_header(envelope.mapping_version, &name))
                .then(|| value.as_str().map(|value| (name, value.to_owned())))
                .flatten()
        })
        .collect())
}

pub(super) fn is_private_pricing_header(mapping_version: &str, name: &str) -> bool {
    match mapping_version {
        // Exact normalized headers from the user-confirmed Product Master.
        // Adding another commercial field requires an explicit mapping-version
        // change; substring matching could accidentally expose unrelated notes.
        "airtek-basic-v1" => matches!(
            name,
            "样品报价sample" | "100500pcs" | "5001000pcs" | "10005000pcs" | "5000pcs"
        ),
        // Minimal synthetic contract mapping used only by unit tests.
        "v1" => matches!(name, "price" | "cost" | "currency"),
        _ => false,
    }
}
