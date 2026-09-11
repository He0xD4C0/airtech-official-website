#!/bin/sh
set -eu

until mc alias set local http://minio:9000 "$MINIO_ROOT_USER" "$MINIO_ROOT_PASSWORD" >/dev/null 2>&1; do
  sleep 2
done

if [ "$AIRTEK_MEDIA_S3_ACCESS_KEY_ID" = "$MINIO_ROOT_USER" ]; then
  echo "The application MinIO access key must not be the root user." >&2
  exit 1
fi

mc mb --ignore-existing "local/$AIRTEK_MEDIA_S3_BUCKET"
mc anonymous set none "local/$AIRTEK_MEDIA_S3_BUCKET"

cat >/tmp/airtek-media-policy.json <<EOF
{
  "Version": "2012-10-17",
  "Statement": [
    {
      "Effect": "Allow",
      "Action": ["s3:GetBucketLocation", "s3:ListBucket"],
      "Resource": ["arn:aws:s3:::$AIRTEK_MEDIA_S3_BUCKET"]
    },
    {
      "Effect": "Allow",
      "Action": ["s3:GetObject", "s3:PutObject", "s3:DeleteObject"],
      "Resource": ["arn:aws:s3:::$AIRTEK_MEDIA_S3_BUCKET/*"]
    }
  ]
}
EOF

mc admin policy create local airtek-media /tmp/airtek-media-policy.json
if ! mc admin user info local "$AIRTEK_MEDIA_S3_ACCESS_KEY_ID" >/dev/null 2>&1; then
  mc admin user add local "$AIRTEK_MEDIA_S3_ACCESS_KEY_ID" "$AIRTEK_MEDIA_S3_SECRET_ACCESS_KEY"
fi
mc admin policy attach local airtek-media --user "$AIRTEK_MEDIA_S3_ACCESS_KEY_ID"

mc alias set app http://minio:9000 "$AIRTEK_MEDIA_S3_ACCESS_KEY_ID" "$AIRTEK_MEDIA_S3_SECRET_ACCESS_KEY" >/dev/null
mc stat "app/$AIRTEK_MEDIA_S3_BUCKET" >/dev/null

echo "Private MinIO media bucket and scoped application user are ready."
