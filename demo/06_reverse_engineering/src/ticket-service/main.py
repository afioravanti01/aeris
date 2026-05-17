from fastapi import FastAPI, HTTPException
from fastapi.middleware.cors import CORSMiddleware
from models import Ticket, TicketCreate
import uuid

app = FastAPI(title="Ticket Service")

app.add_middleware(CORSMiddleware, allow_origins=["*"], allow_methods=["*"], allow_headers=["*"])

tickets: dict[str, Ticket] = {}

@app.get("/tickets")
def list_tickets():
    return list(tickets.values())

@app.get("/tickets/{ticket_id}")
def get_ticket(ticket_id: str):
    if ticket_id not in tickets:
        raise HTTPException(404, "Not found")
    return tickets[ticket_id]

@app.post("/tickets", status_code=201)
def create_ticket(body: TicketCreate):
    ticket = Ticket(id=str(uuid.uuid4()), **body.dict(), status="open")
    tickets[ticket.id] = ticket
    return ticket

@app.patch("/tickets/{ticket_id}/close")
def close_ticket(ticket_id: str):
    if ticket_id not in tickets:
        raise HTTPException(404, "Not found")
    tickets[ticket_id].status = "closed"
    return tickets[ticket_id]

@app.get("/health")
def health():
    return {"status": "ok"}
