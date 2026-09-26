use super::*;

impl Checker {
    pub(super) async fn check_site_assets(&mut self) -> Result<(), PublicReadinessError> {
        let icon = self
            .response(Surface::Public, "/site-icon", Method::GET)
            .await?;
        let mut actual_icon: Option<(String, u32, u32)> = None;
        let fallback = icon.status() == StatusCode::OK;
        if fallback {
            let content_type = content_type(icon.headers());
            require(
                content_type.starts_with("image/svg+xml"),
                "fallback site icon must be SVG",
            )?;
            let body = icon.text().await.map_err(failure)?.to_ascii_lowercase();
            require(
                !body.contains("airtekpower") && !body.contains("logo"),
                "fallback icon must remain neutral",
            )?;
            self.warnings
                .push("No custom site icon is published; the neutral fallback is active.".into());
        } else {
            require(
                icon.status() == StatusCode::PERMANENT_REDIRECT,
                "site icon must return 200 or 308",
            )?;
            let location = icon
                .headers()
                .get(LOCATION)
                .and_then(|value| value.to_str().ok());
            let location = location
                .ok_or_else(|| PublicReadinessError("site icon redirect has no target".into()))?;
            let mut url = self.config.public_origin.join(location).map_err(failure)?;
            let mut delivered = None;
            for _ in 0..5 {
                require(
                    url.scheme() == "https" || self.config.allow_http,
                    "site icon delivery must use HTTPS",
                )?;
                let response = if url.origin() == self.config.public_origin.origin() {
                    self.response(Surface::Public, url.path(), Method::GET)
                        .await?
                } else if url.origin() == self.config.api_origin.origin() {
                    self.response(Surface::Api, url.path(), Method::GET).await?
                } else {
                    self.client.get(url.clone()).send().await.map_err(failure)?
                };
                if response.status().is_redirection() {
                    let target = response
                        .headers()
                        .get(LOCATION)
                        .and_then(|v| v.to_str().ok())
                        .ok_or_else(|| {
                            PublicReadinessError("icon redirect target missing".into())
                        })?;
                    url = url.join(target).map_err(failure)?;
                    continue;
                }
                require(
                    response.status().is_success(),
                    "published icon asset is unreadable",
                )?;
                delivered = Some(response);
                break;
            }
            let mut response =
                delivered.ok_or_else(|| PublicReadinessError("icon redirect loop".into()))?;
            let mime = content_type(response.headers())
                .split(';')
                .next()
                .unwrap_or("")
                .to_owned();
            let mut bytes = Vec::new();
            while let Some(chunk) = response.chunk().await.map_err(failure)? {
                require(
                    bytes.len() + chunk.len() <= 10 * 1024 * 1024,
                    "site icon exceeds 10 MiB",
                )?;
                bytes.extend_from_slice(&chunk);
            }
            let format = image::guess_format(&bytes).map_err(failure)?;
            require(
                format.to_mime_type() == mime,
                "site icon MIME does not match actual bytes",
            )?;
            let (width, height) = image::ImageReader::new(std::io::Cursor::new(bytes))
                .with_guessed_format()
                .map_err(failure)?
                .into_dimensions()
                .map_err(failure)?;
            require(
                width == height && width >= 512,
                "published site icon must be square and at least 512 pixels",
            )?;
            actual_icon = Some((mime, width, height));
        }
        let favicon = self
            .response(Surface::Public, "/favicon.ico", Method::GET)
            .await?;
        require(
            favicon.status() == StatusCode::OK
                || favicon.status() == StatusCode::PERMANENT_REDIRECT,
            "favicon endpoint is not readable",
        )?;
        let manifest_response = self
            .response(Surface::Public, "/site.webmanifest", Method::GET)
            .await?;
        require(
            manifest_response.status().is_success(),
            "site.webmanifest is unavailable",
        )?;
        require(
            content_type(manifest_response.headers()).starts_with("application/manifest+json"),
            "site.webmanifest has the wrong media type",
        )?;
        let manifest: Value = manifest_response.json().await.map_err(failure)?;
        let manifest_icon = manifest.pointer("/icons/0").and_then(Value::as_object);
        require(
            manifest_icon
                .and_then(|icon| icon.get("src"))
                .and_then(Value::as_str)
                == Some("/site-icon"),
            "site.webmanifest does not reference /site-icon",
        )?;
        let (mime, sizes) = actual_icon
            .map(|(mime, width, height)| (mime, format!("{width}x{height}")))
            .unwrap_or_else(|| ("image/svg+xml".into(), "any".into()));
        require(
            manifest.pointer("/icons/0/type").and_then(Value::as_str) == Some(mime.as_str())
                && manifest.pointer("/icons/0/sizes").and_then(Value::as_str)
                    == Some(sizes.as_str()),
            "manifest icon metadata differs from the delivered asset",
        )?;
        self.checks.push("siteIconAndManifest");
        Ok(())
    }
}
