from fastapi import FastAPI, HTTPException
from pydantic import BaseModel
from typing import Optional
import uuid

app = FastAPI(title="Products Service")

db = {
    "p1": {"id": "p1", "name": "Widget",     "price": 9.99,  "stock": 100, "category": "tools"},
    "p2": {"id": "p2", "name": "Gadget",     "price": 29.99, "stock": 50,  "category": "electronics"},
    "p3": {"id": "p3", "name": "Doohickey",  "price": 4.99,  "stock": 200, "category": "tools"},
    "p4": {"id": "p4", "name": "Thingamajig","price": 14.99, "stock": 75,  "category": "misc"},
}

class CreateProduct(BaseModel):
    name: str
    price: float
    stock: Optional[int] = 0
    category: Optional[str] = "misc"

@app.get("/products")
def list_products(category: Optional[str] = None):
    items = list(db.values())
    if category:
        items = [p for p in items if p["category"] == category]
    return items

@app.get("/products/{product_id}")
def get_product(product_id: str):
    product = db.get(product_id)
    if not product:
        raise HTTPException(status_code=404, detail=f"Product '{product_id}' not found")
    return product

@app.post("/products", status_code=201)
def create_product(payload: CreateProduct):
    pid = "p" + str(uuid.uuid4())[:6]
    product = {"id": pid, **payload.model_dump()}
    db[pid] = product
    return product

@app.patch("/products/{product_id}/stock")
def update_stock(product_id: str, delta: int):
    product = db.get(product_id)
    if not product:
        raise HTTPException(status_code=404, detail=f"Product '{product_id}' not found")
    product["stock"] = max(0, product["stock"] + delta)
    return product
