import React from 'react';
import { BrowserRouter, Routes, Route, Link } from 'react-router-dom';
import TicketList   from './components/TicketList';
import TicketForm   from './components/TicketForm';
import TicketDetail from './components/TicketDetail';

export default function App() {
  return (
    <BrowserRouter>
      <nav style={{ padding: '12px 24px', background: '#1e40af', color: 'white', display: 'flex', gap: 20 }}>
        <Link to="/" style={{ color: 'white', fontWeight: 'bold' }}>🎫 Ticketing</Link>
        <Link to="/" style={{ color: '#bfdbfe' }}>Tickets</Link>
        <Link to="/new" style={{ color: '#bfdbfe' }}>New</Link>
      </nav>
      <main style={{ padding: 24 }}>
        <Routes>
          <Route path="/"              element={<TicketList />} />
          <Route path="/new"           element={<TicketForm />} />
          <Route path="/tickets/:id"   element={<TicketDetail />} />
        </Routes>
      </main>
    </BrowserRouter>
  );
}
