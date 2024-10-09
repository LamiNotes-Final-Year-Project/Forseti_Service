# Forseti_Service
Forseti is a rust based web service that is designed to handle concurrent writing to a document, and push those changes down to a team of collaborators 

## Dependencies:
Forsetti is built using light amounts of frameworks that give base functionality to the app, I am opting to stay away from large exisiting frameworks due to their unnecessary nature and reiteration of my learning.

- Actix Web: https://github.com/actix/actix-web
  Web framework for rust that handles RESTful functionality and web socketing, also comes with Tokio package, allowing asynchronous requests for concurrency
- JWT auth: https://docs.rs/jsonwebtoken/latest/jsonwebtoken/index.html
  JSON Web Token authentication for securely handling data across requests.



