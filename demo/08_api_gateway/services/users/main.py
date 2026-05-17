from fastapi import FastAPI, HTTPException
from pydantic import BaseModel
from typing import Optional
import uuid

app = FastAPI(title="Users Service")

db = {
    "u1": {"id": "u1", "name": "Alice",   "email": "alice@example.com",   "role": "admin"},
    "u2": {"id": "u2", "name": "Bob",     "email": "bob@example.com",     "role": "user"},
    "u3": {"id": "u3", "name": "Charlie", "email": "charlie@example.com", "role": "user"},
}

class CreateUser(BaseModel):
    name: str
    email: str
    role: Optional[str] = "user"

@app.get("/users")
def list_users():
    return list(db.values())

@app.get("/users/{user_id}")
def get_user(user_id: str):
    user = db.get(user_id)
    if not user:
        raise HTTPException(status_code=404, detail=f"User '{user_id}' not found")
    return user

@app.post("/users", status_code=201)
def create_user(payload: CreateUser):
    uid = "u" + str(uuid.uuid4())[:6]
    user = {"id": uid, **payload.model_dump()}
    db[uid] = user
    return user

@app.delete("/users/{user_id}", status_code=204)
def delete_user(user_id: str):
    if user_id not in db:
        raise HTTPException(status_code=404, detail=f"User '{user_id}' not found")
    del db[user_id]
