# StrIEM

**Streaming Intelligence and Event Management**

StrIEM is an open-source Security Information and Event Management (SIEM) platform built on [Vector](https://vector.dev), the high-performance observability data pipeline. It provides real-time Sigma rule detection on streaming security data, automatic normalization to the [Open Cybersecurity Schema Framework (OCSF)](https://ocsf.io), and flexible storage options including local Parquet files or cloud security data lakes.

## Key Features

- **Real-time Sigma Detection**: Evaluate [Sigma rules](https://github.com/SigmaHQ/sigma) on streaming data with millisecond latency
- **OCSF Normalization**: Automatic transformation of security logs to OCSF standard schema
- **High Performance**: Built on Vector's Rust-based pipeline for maximum throughput
- **Flexible Storage**: 
  - Local Parquet files for cost-effective storage
  - Direct integration with S3, Snowflake, and other data lakes
- **SQL Querying**: Built-in DuckDB integration for fast SQL queries on stored data
- **Management UI**: Web interface for managing sources, viewing alerts, and querying data
- **Multi-Source Support**: AWS CloudTrail, Okta, GitHub, Google Cloud, and more

## Architecture

```
┌─────────────┐     ┌──────────────┐     ┌─────────────┐
│   Sources   │────▶│    Vector    │────▶│   StrIEM    │
│ (CloudTrail,│     │  (Streaming  │     │  (Detection │
│  Okta, etc) │     │  & Transform)│     │  & Storage) │
└─────────────┘     └──────────────┘     └─────────────┘
                           │                     │
                           │                     ▼
                           │              ┌─────────────┐
                           │              │  Parquet    │
                           └─────────────▶│  Storage    │
                                          │ (OCSF Data) │
                                          └─────────────┘
```

**How StrIEM Works:**

1. **Configuration Generation**: StrIEM generates a Vector configuration based on defined sources
2. **Data Ingestion**: Vector collects logs from configured sources (AWS, Okta, etc.)
3. **Normalization**: VRL (Vector Remap Language) scripts transform data to OCSF format
4. **Streaming Detection**: StrIEM daemon receives events via gRPC and evaluates Sigma rules
5. **Storage**: Events are buffered and written as Parquet files organized by OCSF class
6. **Querying**: DuckDB provides fast SQL queries directly on Parquet files

## Quick Start

### Prerequisites

- Docker and Docker Compose
- Or: Rust toolchain (latest stable) and Node.js 20+

### Running with Docker Compose

The fastest way to get started is using Docker Compose:

```bash
# Clone the repository
git clone https://github.com/sonnens/striem.git
cd striem

# Clone OCSF VRL transforms for data normalization
git clone https://github.com/crowdalert/ocsf-vrl.git data/remaps

# Start all services
docker-compose up -d

# Access the UI
open http://localhost:8080/ui
```

This will start:
- **StrIEM** on port 8080 (API + UI)
- **Vector** on port 9000 (gRPC), 8000 (metrics), 8008 (health)

### Building from Source

```bash
# Clone the repository
git clone https://github.com/sonnens/striem.git
cd striem

# Clone OCSF VRL transforms for data normalization
git clone https://github.com/crowdalert/ocsf-vrl.git data/remaps

# Build the Rust backend
cargo build --release

# Build the UI
cd ui
npm install
npm run build
cd ..

# Run StrIEM
./target/release/striem
```

## Configuration

StrIEM can be configured via YAML, TOML, JSON or environment variables.

### Configuration File Example

Create a `config.yaml`:

```yaml
# Detection rules directory
detections: ./data/detections

# Input configuration (Vector → StrIEM)
input:
  vector:
    address: 0.0.0.0:3000

# Output configuration (StrIEM → Vector)
output:
  vector:
    url: http://localhost:9000

# Storage configuration
storage:
  schema: ./data/schema/1.4.0
  path: ./data/storage

# API configuration
api:
  address: 0.0.0.0:8080
  data_dir: ./data/db
  ui_path: ./ui/out
```

Run with config file:
```bash
striem config.yaml
```

### Environment Variables

All configuration options can be set via environment variables with the `STRIEM_` prefix:

```bash
export STRIEM_DETECTIONS=/path/to/sigma/rules
export STRIEM_API_ADDRESS=0.0.0.0:8080
export STRIEM_INPUT_VECTOR_ADDRESS=0.0.0.0:3000
export STRIEM_OUTPUT_VECTOR_URL=http://localhost:9000
export STRIEM_STORAGE_SCHEMA=/data/schema/1.4.0
export STRIEM_STORAGE_PATH=/data/storage
export STRIEM_REMAPS=/data/remaps

./target/release/striem
```

## Project Structure

```
striem/
├── src/                    # Main Rust application
│   ├── main.rs            # Entry point
│   ├── app.rs             # Application orchestration
│   └── detection.rs       # Sigma rule evaluation
├── lib/
│   ├── api/               # REST API and management interface
│   ├── common/            # Shared types and utilities
│   ├── config/            # Configuration management
│   ├── storage/           # Parquet storage backend
│   └── vector/            # Vector gRPC client/server
├── ui/                    # Next.js management interface
│   ├── app/
│   │   ├── components/    # React components
│   │   │   ├── Rules/     # Sigma rules management
│   │   │   ├── Sources/   # Data source configuration
│   │   │   └── Explore/   # Data querying interface
│   │   └── api/          # API routes
│   └── include/types/     # TypeScript definitions
├── data/
│   ├── detections/        # Sigma rule YAML files
│   ├── remaps/            # OCSF VRL transformation scripts
│   ├── schema/            # OCSF schema definitions
│   └── storage/           # Parquet file output
└── docker-compose.yaml    # Complete stack deployment
```

## Management Interface

The web UI provides:

### Sources Management
- Add/remove data sources (AWS CloudTrail, Okta, etc.)
- Configure source-specific parameters
- Enable/disable sources
- Monitor source status

### Detection Rules
- View loaded Sigma rules
- Upload new YAML rule files
- Enable/disable individual rules
- Filter by severity, product, service

### Data Explorer
- Query stored data with DuckDB SQL
- Filter by time range and OCSF class
- Export query results
- View detection alerts

## Adding Data Sources

StrIEM supports multiple security data sources out of the box.

### AWS CloudTrail

Via the UI or API:

```bash
curl -X POST http://localhost:8080/api/1/sources/aws_cloudtrail \
  -H "Content-Type: application/json" \
  -d '{
    "name": "Production CloudTrail",
    "region": "us-east-1",
    "sqs": {
      "queue_url": "https://sqs.us-east-1.amazonaws.com/123456789/cloudtrail-queue",
      "delete_message": true,
      "poll_secs": 15
    }
  }'
```

### Okta

```bash
curl -X POST http://localhost:8080/api/1/sources/okta \
  -H "Content-Type: application/json" \
  -d '{
    "name": "Okta System Logs",
    "domain": "your-domain.okta.com",
    "token": "your-api-token"
  }'
```

## Querying Data

### Using the UI

Navigate to the "Explore" tab and run SQL queries:

```sql
-- View recent authentication events
SELECT * FROM read_parquet('/data/storage/iam/authentication/**/*.parquet')
WHERE time > now() - interval '24 hours'
LIMIT 100;

-- Count detections by severity
SELECT 
  metadata->>'severity' as severity,
  COUNT(*) as count
FROM read_parquet('/data/storage/findings/detection_finding/**/*.parquet')
GROUP BY severity
ORDER BY count DESC;
```

### Using DuckDB CLI

```bash
# Connect to the database
duckdb

# Query Parquet files directly
SELECT * FROM read_parquet('/data/storage/**/*.parquet') 
WHERE class_uid = 3003 
LIMIT 10;
```

## Detection Rules

StrIEM uses [Sigma rules](https://github.com/SigmaHQ/sigma) for threat detection.

### Adding Rules

Place Sigma YAML files in the `data/detections/` directory, or upload via the UI:

```yaml
# example-detection.yaml
title: Suspicious PowerShell Execution
description: Detects suspicious PowerShell command execution
status: experimental
logsource:
  product: windows
  service: powershell
detection:
  selection:
    EventID: 4104
    ScriptBlockText|contains:
      - 'Invoke-Mimikatz'
      - 'Invoke-Expression'
  condition: selection
level: high
```

### Rule Management

- **Upload**: Click "Upload" button in Rules tab
- **Enable/Disable**: Toggle rules on/off without deletion
- **Filter**: Search by level, product, service, or description

## OCSF Normalization

StrIEM automatically normalizes data to OCSF format using VRL scripts.

### Supported OCSF Classes

- **IAM**: Authentication (3002), Authorization (3003)
- **Network**: Network Activity (4001), HTTP Activity (4002)
- **System**: Process Activity (1007), File Activity (1001)
- **Findings**: Detection Finding (2004), Security Finding (2001)

### Custom Remaps

Create custom VRL remaps in `data/remaps/{source}/remap.vrl`:

```ruby
# data/remaps/custom_source/remap.vrl
.class_uid = 3002  # Authentication
.time = to_unix_timestamp(to_timestamp!(.timestamp))
.user.name = .username
.src_endpoint.ip = .source_ip
```

## 🚢 Production Deployment

### Docker Deployment

```yaml
# docker-compose.yml
services:
  striem:
    image: striem:latest
    environment:
      - STRIEM_API_ADDRESS=0.0.0.0:8080
      - STRIEM_DETECTIONS=/data/detections
      - STRIEM_STORAGE_PATH=/data/storage
    volumes:
      - ./data:/data
    ports:
      - "8080:8080"

  vector:
    image: timberio/vector:nightly-distroless-libc
    volumes:
      - ./extra/vector.yaml:/etc/vector/vector.yaml:ro
      - ./data:/data:ro
    ports:
      - "9000:9000"
```

### Kubernetes

See `k8s/` directory for Kubernetes manifests (coming soon).

## Security Considerations

- **Authentication**: Currently no built-in authentication (use reverse proxy)
- **API Keys**: Secure source credentials in environment variables
- **Network**: Run on internal networks or behind VPN
- **Data**: Parquet files contain sensitive security logs - secure storage appropriately

## Development

### Building

```bash
# Backend
cargo build

# Frontend
cd ui && npm install && npm run dev

# Run tests
cargo test
```

### Adding a New Source

1. Create source module in `lib/api/src/sources/`
2. Implement the `Source` trait
3. Add VRL remap in `data/remaps/{source}/`
4. Register in `lib/api/src/sources/mod.rs`

See existing sources (AWS CloudTrail, Okta) for examples.

## Documentation

- [Vector Documentation](https://vector.dev/docs/)
- [Sigma Rules](https://github.com/SigmaHQ/sigma)
- [OCSF Schema](https://schema.ocsf.io/)
- [VRL Language Guide](https://vector.dev/docs/reference/vrl/)
- [DuckDB SQL](https://duckdb.org/docs/sql/introduction)

## Contributing

Contributions are welcome! Please:

1. Fork the repository
2. Create a feature branch
3. Make your changes
4. Add tests if applicable
5. Submit a pull request

### Areas for Contribution

- New data source integrations
- OCSF VRL remaps for additional sources
- Additional Sigma rules
- UI/UX improvements
- Documentation and examples

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

## Acknowledgments

- [Vector](https://vector.dev) - High-performance data pipeline
- [Sigma](https://github.com/SigmaHQ/sigma) - Generic signature format for SIEM systems
- [OCSF](https://ocsf.io) - Open Cybersecurity Schema Framework
- [DuckDB](https://duckdb.org) - In-process SQL OLAP database
- [Apache Parquet](https://parquet.apache.org) - Columnar storage format

## Support

- **Issues**: [GitHub Issues](https://github.com/sonnens/striem/issues)
- **Discussions**: [GitHub Discussions](https://github.com/sonnens/striem/discussions)
