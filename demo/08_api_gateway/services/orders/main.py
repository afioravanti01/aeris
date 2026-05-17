from fastapi import FastAPI, HTTPException
from pydantic import BaseModel
from typing import Optional
import uuid, datetime

app = FastAPI(title="Orders Service")

db = {}

VALID_STATUSES = {"pending", "confirmed", "shipped", "delivered", "cancelled"}

class CreateOrder(BaseModel):
    user_id: str
    product_id: str
    quantity: int

class UpdateStatus(BaseModel):
    status: str

@app.get("/orders")
def list_orders(user_id: Optional[str] = None, status: Optional[str] = None):
    items = list(db.values())
    if user_id:
        items = [o for o in items if o["user_id"] == user_id]
    if status:
        items = [o for o in items if o["status"] == status]
    return items

@app.get("/orders/{order_id}")
def get_order(order_id: str):
    order = db.get(order_id)
    if not order:
        raise HTTPException(status_code=404, detail=f"Order '{order_id}' not found")
    return order

@app.post("/orders", status_code=201)
def create_order(payload: CreateOrder):
    oid = "o" + str(uuid.uuid4())[:6]
    order = {
        "id":         oid,
        "user_id":    payload.user_id,
        "product_id": payload.product_id,
        "quantity":   payload.quantity,
        "status":     "pending",
        "created_at": datetime.datetime.utcnow().isoformat() + "Z",
    }
    db[oid] = order
    return order

@app.patch("/orders/{order_id}/status")
def update_order_status(order_id: str, payload: UpdateStatus):
    order = db.get(order_id)
    if not order:
        raise HTTPException(status_code=404, detail=f"Order '{order_id}' not found")
    if payload.status not in VALID_STATUSES:
        raise HTTPException(status_code=400, detail=f"Invalid status '{payload.status}'")
    order["status"] = payload.status
    return order

@app.delete("/orders/{order_id}", status_code=204)
def cancel_order(order_id: str):
    order = db.get(order_id)
    if not order:
        raise HTTPException(status_code=404, detail=f"Order '{order_id}' not found")
    order["status"] = "cancelled"
