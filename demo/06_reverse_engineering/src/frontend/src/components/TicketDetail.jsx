import React, { useEffect, useState } from 'react';
import { useParams, useNavigate } from 'react-router-dom';
import { api } from '../api/client';

export default function TicketDetail() {
  const { id }         = useParams();
  const navigate       = useNavigate();
  const [ticket,  setTicket]  = useState(null);
  const [comment, setComment] = useState('');
  const [loading, setLoading] = useState(true);
  const [error,   setError]   = useState(null);

  const load = () =>
    api.tickets.get(id)
      .then(r => setTicket(r.data))
      .catch(e => setError(e.message))
      .finally(() => setLoading(false));

  useEffect(() => { load(); }, [id]);

  const close = async () => {
    await api.tickets.close(id);
    load();
  };

  const sendComment = async (e) => {
    e.preventDefault();
    if (!comment.trim()) return;
    await api.tickets.comment(id, comment);
    setComment('');
    load();
  };

  if (loading) return <p>Loading...</p>;
  if (error)   return <p style={{ color: 'red' }}>Error: {error}</p>;
  if (!ticket) return <p>Ticket not found.</p>;

  return (
    <div style={{ maxWidth: 700 }}>
      <button onClick={() => navigate(-1)}>← Back</button>
      <h2>[#{ticket.id}] {ticket.title}</h2>

      <div style={{ display: 'flex', gap: 24, color: '#6b7280', fontSize: 14, marginBottom: 16 }}>
        <span>Status: <strong>{ticket.status}</strong></span>
        <span>Priority: <strong>{ticket.priority}</strong></span>
        <span>Assignee: <strong>{ticket.assignee ?? '—'}</strong></span>
        <span>Created: <strong>{new Date(ticket.created_at).toLocaleString('en-GB')}</strong></span>
      </div>

      <p style={{ whiteSpace: 'pre-wrap', background: '#f9fafb', padding: 12, borderRadius: 4 }}>
        {ticket.description || <em>No description.</em>}
      </p>

      {ticket.status !== 'closed' && (
        <button onClick={close} style={{ marginBottom: 24 }}>Close ticket</button>
      )}

      <h3>Comments ({ticket.comments?.length ?? 0})</h3>
      {(ticket.comments ?? []).map((c, i) => (
        <div key={i} style={{ border: '1px solid #e5e7eb', borderRadius: 4, padding: 12, marginBottom: 8 }}>
          <div style={{ fontSize: 12, color: '#6b7280' }}>{c.author} · {new Date(c.created_at).toLocaleString('en-GB')}</div>
          <p style={{ margin: '4px 0 0' }}>{c.text}</p>
        </div>
      ))}

      {ticket.status !== 'closed' && (
        <form onSubmit={sendComment} style={{ marginTop: 16 }}>
          <textarea
            value={comment}
            onChange={e => setComment(e.target.value)}
            rows={3}
            placeholder="Add a comment..."
            style={{ width: '100%' }}
          />
          <button type="submit" style={{ marginTop: 8 }}>Submit comment</button>
        </form>
      )}
    </div>
  );
}
