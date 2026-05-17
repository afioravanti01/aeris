# Project Template

## Name
users-ms

## Description
Java microservice for user management

## Stack
- Language: Java 21
- Framework: Springboot
- Database: in-memory
- Package: com.afioravanti.examples

## Project Layers

- Controller
- Service
- Repository
- Model

## Deployment

- Kubernetes
- Manifests
    - k8s/config.yml
    - k8s/deployment.yml
    - k8s/ingress.yml
    - k8s/service.yml

## Endpoints
- GET /users — list all users
- POST /users — create a user
- GET /users/{id} — get a user by id
- DELETE /items/{id} — delete a user

## Additional requirements
- CORS enabled
- Health check at /health
- README with startup instructions
- Maven profiles: dev, local
