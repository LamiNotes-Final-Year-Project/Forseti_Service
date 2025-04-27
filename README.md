# Forseti Service

Forseti is a Rust-based backend service that powers the Laminotes collaborative markdown editor. It provides version control, team collaboration, and conflict resolution capabilities through a RESTful API.

## Key Features

- **User Authentication**: Secure JWT-based authentication system
- **Version Control**: Track document history and facilitate collaborative editing
- **Team Management**: Create teams and manage member permissions
- **Conflict Resolution**: Detect and help resolve conflicting changes
- **File System Integration**: Store and manage files with structured metadata
- **API Endpoints**: RESTful API for frontend integration
- **Claude AI Integration**: Connect to Claude API for AI-assisted features

## Architecture

Forseti is built using a modular architecture with the following components:

- **Routes**: API endpoint handlers organized by functionality
- **Models**: Data structures for application entities
- **Services**: Business logic implementation
- **Utils**: Helper functions and middleware
- **Storage**: File system handlers and abstractions

## Dependencies

Forseti is built with the following key dependencies:

- **Actix Web**: Web framework for Rust with excellent performance characteristics
- **Tokio**: Asynchronous runtime for handling concurrent operations
- **JWT Auth**: JSON Web Token authentication for secure API access
- **Serde JSON**: Serialization/deserialization for JSON handling
- **Chrono**: Date and time handling
- **UUID**: Unique identifier generation
- **reqwest**: HTTP client for external API interactions (Claude API)

## Installation & Setup

### Prerequisites

- Rust 1.70+ and Cargo
- System dependencies: OpenSSL development libraries

### Building from Source

```bash
# Clone the repository
git clone https://github.com/yourusername/forseti-service.git

# Navigate to the project directory
cd forseti-service

# Build the project
cargo build --release

# Run the server
cargo run --release
```

## API Endpoints

### Authentication

- `POST /auth/register`: Register a new user
- `POST /auth/login`: Login and retrieve JWT token
- `GET /auth/validate`: Validate JWT token

### Files

- `GET /files`: List all files for the current user
- `GET /files/:id`: Get a specific file by ID
- `POST /files`: Create a new file
- `PUT /files/:id`: Update a file
- `DELETE /files/:id`: Delete a file
- `GET /files/:id/history`: Get version history for a file

### Teams

- `GET /teams`: List all teams for the current user
- `POST /teams`: Create a new team
- `GET /teams/:id`: Get a specific team by ID
- `GET /teams/:id/members`: List all members of a team
- `POST /teams/:id/members`: Add a new member to a team
- `DELETE /teams/:id/members/:user_id`: Remove a member from a team

### Invitations

- `POST /invitations`: Create a new team invitation
- `GET /invitations`: List all invitations for the current user
- `PUT /invitations/:id/accept`: Accept an invitation
- `PUT /invitations/:id/decline`: Decline an invitation

### Version Control

- `POST /files/:id/save`: Save a new version of a file
- `GET /files/:id/versions`: Get all versions of a file
- `GET /files/:id/versions/:version_id`: Get a specific version of a file
- `POST /files/:id/resolve-conflicts`: Resolve conflicts in a file

### Claude AI Integration

- `POST /claude/completion`: Get a completion from Claude AI
- `POST /claude/process-markdown`: Process markdown with Claude AI

## Configuration

Forseti can be configured through environment variables:

- `FORSETI_HOST`: Host address to bind to (default: 127.0.0.1)
- `FORSETI_PORT`: Port to listen on (default: 9090)
- `JWT_SECRET`: Secret key for JWT token generation
- `STORAGE_PATH`: Path to file storage directory
- `CLAUDE_API_KEY`: API key for Claude AI integration
- `LOG_LEVEL`: Logging level (default: info)

## File Storage Structure

Forseti uses a structured file system for storage:

```
storage/
├── users/                 # User metadata
├── teams/                 # Team metadata and team-specific files
├── team_members/          # Team membership information
├── invitations/           # Team invitations
└── versions/              # File version history
```

## Collaborative Editing

Forseti implements a version-based collaborative editing system:

1. Files are stored with metadata including owner, team, and current version
2. When saving, clients provide the base version they started with
3. Server checks for conflicts (newer versions created by other users)
4. If conflicts exist, detailed conflict information is returned
5. Client can resolve conflicts and submit the resolved content

## Security

- JWT tokens for authentication with configurable expiration
- Role-based access control for team resources
- Content validation and sanitization
- Proper error handling to prevent information leakage

## Extensions and Integrations

- **Claude AI**: Integration with Anthropic's Claude AI for intelligent features
- **Cross-platform Support**: Works with web, desktop, and mobile clients
- **Offline Capabilities**: Support for syncing local changes when online

## Development

### Running Tests

```bash
# Run all tests
cargo test

# Run specific test
cargo test test_version_control
```

### Building Documentation

```bash
# Generate documentation
cargo doc --no-deps --open
```

the user will aslo need  .env file with the following variables:
`JWT_SECRET`

`CLAUDE_API_KEY`


## License

This project is licensed under the MIT License - see the LICENSE file for details.
