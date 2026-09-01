CREATE TABLE public_rate_limits (
    scope text NOT NULL,
    source_hash text NOT NULL,
    window_started_at timestamptz NOT NULL,
    request_count integer NOT NULL CHECK (request_count > 0),
    expires_at timestamptz NOT NULL,
    PRIMARY KEY (scope, source_hash)
);

CREATE INDEX public_rate_limits_expiry_idx ON public_rate_limits (expires_at);
