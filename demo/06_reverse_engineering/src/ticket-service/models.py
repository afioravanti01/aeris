from pydantic import BaseModel
from typing import Optional

class TicketCreate(BaseModel):
    title:       str
    description: str = ""
    priority:    str = "medium"

class Ticket(TicketCreate):
    id:     str
    status: str = "open"
