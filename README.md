# Timefusion

A very specialized timeseries database created for events, logs, traces and metrics.

Its designed to allow users plug in their own s3 storage and buckets and have their stored to their accounts.
This way, timefusion is used as a compute and cache engine, not primary data storage.

Timefusion speaks the postgres dialect, so you can insert and read from it using any postgres client or driver.

## Configuration

Timefusion can be configured using the following environment variables:

| Variable              | Description                   | Default                    |
| --------------------- | ----------------------------- | -------------------------- |
| `PORT`                | HTTP server port              | `80`                       |
| `PGWIRE_PORT`         | PostgreSQL wire protocol port | `5432`                     |
| `AWS_S3_BUCKET`       | AWS S3 bucket name            | Required                   |
| `AWS_S3_ENDPOINT`     | AWS S3 endpoint URL           | `https://s3.amazonaws.com` |
| AWS_ACCESS_KEY_ID     | AWS access key                | -                          |
| AWS_SECRET_ACCESS_KEY | AWS secret key                | -                          |

For local development, you can set `QUEUE_DB_PATH` to a location in your development environment.
