# API Endpoints

Reference for all API endpoints.

## Users

### List Users

```http
GET /api/v1/users
```

Returns a list of all users.

**Response:**

```json
{
  "users": [
    { "id": 1, "name": "Alice" },
    { "id": 2, "name": "Bob" }
  ]
}
```

### Get User

```http
GET /api/v1/users/:id
```

Returns a single user by ID.

## Resources

### Create Resource

```http
POST /api/v1/resources
```

Creates a new resource.

**Request Body:**

| Field      | Type   | Required | Description         |
| ---------- | ------ | -------- | ------------------- |
| `name`     | string | Yes      | Resource name       |
| `type`     | string | Yes      | Resource type       |
| `metadata` | object | No       | Additional metadata |

## Rate Limits

| Tier       | Requests/min | Requests/day |
| ---------- | ------------ | ------------ |
| Free       | 60           | 1,000        |
| Pro        | 300          | 10,000       |
| Enterprise | 1,000        | Unlimited    |

## Error Codes

- `400` - Bad Request
- `401` - Unauthorized
- `403` - Forbidden
- `404` - Not Found
- `429` - Rate Limited
- `500` - Internal Server Error
