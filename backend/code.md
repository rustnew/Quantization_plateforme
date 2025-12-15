fossouomartial@pop-os:~/quantization_plateforme$ cd backend 
fossouomartial@pop-os:~/quantization_plateforme/backend$ cd  src 
fossouomartial@pop-os:~/quantization_plateforme/backend/src$ ls  
api  core  domain  infrastructure  lib.rs  main.rs  workers
fossouomartial@pop-os:~/quantization_plateforme/backend/src$ ls  api
mod.rs  routes
fossouomartial@pop-os:~/quantization_plateforme/backend/src$ ls api/routes
auth.rs  jobs.rs  middleware.rs  models.rs  mod.rs  subscriptions.rs  upload.rs
fossouomartial@pop-os:~/quantization_plateforme/backend/src$ ls core 
mod.rs  quantization
fossouomartial@pop-os:~/quantization_plateforme/backend/src$ ls  core/quantization
analysis.rs  gguf.rs  mod.rs  onnx.rs  pipeline.rs  pytorch.rs  validation.rs
fossouomartial@pop-os:~/quantization_plateforme/backend/src$ ls  domain
jobs.rs  model.rs  mod.rs  user.rs
fossouomartial@pop-os:~/quantization_plateforme/backend/src$ ls  infrastructure 
database  mod.rs  python  queue  storage
fossouomartial@pop-os:~/quantization_plateforme/backend/src$ ls  infrastructure/database 
jobs.rs  mod.rs  subscriptions.rs  users.rs
fossouomartial@pop-os:~/quantization_plateforme/backend/src$ ls  infrastructure/python
awq.rs  gptq.rs  mod.rs
fossouomartial@pop-os:~/quantization_plateforme/backend/src$ ls  infrastructure/queue
mod.rs
fossouomartial@pop-os:~/quantization_plateforme/backend/src$ ls  infrastructure/storage
mod.rs
fossouomartial@pop-os:~/quantization_plateforme/backend/src$ ls  workers
cleanup_worker.rs  mod.rs  quantization_worker.rs
fossouomartial@pop-os:~/quantization_plateforme/backend/src$ cd ..
fossouomartial@pop-os:~/quantization_plateforme/backend$ ls 
Cargo.lock  code.md  docker-compose.yml  migrations  src
Cargo.toml  config   Dockerfile          scripts     target
fossouomartial@pop-os:~/quantization_plateforme/backend$ 
