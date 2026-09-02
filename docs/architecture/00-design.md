# Music Licensing for Movies

## User Types
The application has three user types:
- Movie studio staff
- Record label staff
- Administrators

## Data Model
### Movie
A movie object represents a movie that's being worked on by a film studio. This movie is defined by:
- The studio that is working on this movie
- A title
- A short description
- A set of scenes
- A poster image (which it might not have)
Movies can only be created or updated by their own movie studio's staff.

### Scene
A scene is a part of a movie. It is defined by:
- Screen time (measured in seconds)
- Start and end times (measured in seconds from the start of the film)
- A short description
- A representative media file for the scene, assuming an image for now (it might not have been filmed yet)
Scenes within a movie must not overlap in time.

### Song
Movie studios may license songs for use within their movie's scenes. A song is defined by:
- Record label
- Song title
- Song author
- Box art image
- Length (in seconds)

### License
A license records the progress of a licensing deal for a song, within the context of a movie scene. It is defined by:
- song_id
- movie_id
- scene_id
- start_time (in seconds, from beginning of scene)
- end_time (in seconds, from beginning of scene)
- license_fee
- state (enumerative type)
    - OFFER (movie studio sets an offer)
    - COUNTER_OFFER (record label sets a counter offer)
    - ACCEPTED (either side accepts the current deal: movie studio can only accept COUNTER_OFFERs, record label can only accept OFFERs
    - REJECTED (either side rejects the deal)

### License Log
We keep a log of the license state changes to know where a licensing deal is in its lifecycle, as song licensing follows a multi-step process:
- Movie studio searches for a song to use in their scene
- Movie studio selects song
- License object is created in the database, with an OFFER state
- Record label gets notified
- Record label accepts the offer:
    - License state changes to ACCEPTED
- Record label counter-offers:
    - License state changes to COUNTER_OFFER with new license_fee (set by record label)
    - Movie studio may accept the offer, or offer again.
- Any of the two parties may reject the offer at any time.

## Basic Endpoints
### Movie Studio endpoints
- GET /movies -> Movie[]
- POST /movies {Movie} -> Movie (with ID)
- PUT /movies/:id/scenes {Scene} -> Scene (with ID)
- PUT /movies/:id/poster -> S3 PresignedURL
- PUT /movies/:id/scenes/:scene_id/capture -> S3 PresignedURL

### Record Label endpoints
- POST /song
- PUT /song/:id/box_art -> S3 PresignedURL
- PUT /song/:id/audio_preview -> S3 PresignedURL (max 30s)

### Song Searching
- GET /songs/search?q={search_string} -> Song[]
- GET /songs/:id -> Song

### Licensing
- POST /license {License} -> License (with ID, and state = OFFER)
- PUT /license {state} -> License

## Service Definitions
There are five main services within the architecture:

### Authentication Service
This service is in charge of managing user CRUD, and authentication. When an user authenticates, they're issued a JWT that's checked by the other services.

### Movie Service
This service is in charge of handling movie and scene CRUD operations.

### Song Service
This service is in charge of handling Song CRUD operations.

### Search Service
This service handles search queries for songs.

### Licensing Service
This service handles all operations relating to song licensing, its lifecycles, and notifications to any of the involved parties. This service also handles live licensing state updates for users connected to the application via SSE.

## System Architecture
L7 Load Balancer in front of all services.
Movie Service connects to Postgres instance.
Song Service connects to Postgres instance.
Change data capture via Kafka is set up to hydrate ElasticSearch's song catalog.
Search Service is connected to ElasticSearch (with a Redis cache to handle requests for popular songs)

Redis PubSub is used to transmit events from licensing services to the SSE handles users are connected to.
Licensing services are connected to an external Notification service that sends emails, push notifications or text messages. We can use email for the MVP.

### Usage of this repository
This repository shall be used in the following manner:
- service/
    - api_gateway
    - auth_service
    - movie_service
    - song_service
    - search_service
    - license_service
- frontend/
    - (React application, compiled with Vite to be deployed in a static manner on CloudFront)
- infrastructure/
    - pulumi/
        - (Pulumi codebase to manage compute and storage resources)
    - k8s/
        - (production kubernetes configuration, with SealedSecrets for now)
    - docker/
        - (docker compose stack for local development, with sane defaults)


## Architecture Decisions
- Users will register and sign in with their own credentials, instead of using OAuth2, for the MVP.
- The API surface is defined as RESTful only, for simplicity of implementation (and GraphQL is not necessary for the kind of operations the product requires)
- Media is to be stored on S3 behind CloudFront, uploaded by users via pre-signed URLs.
- Movie and Song IDs are to be UUIDv7 ids, to enable horizontal scaling of movie and song services.
- Scene IDs within a movie may be integers.
- Song searches are to be served via ElasticSearch to lower the load on our RDB. We're fine with the song catalog being eventually consistent.
- All architecture is to be deployed on a Kubernetes cluster, so local container state should be avoided whenever possible.
