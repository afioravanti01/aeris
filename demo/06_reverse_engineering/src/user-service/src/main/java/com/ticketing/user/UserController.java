package com.ticketing.user;

import org.springframework.http.ResponseEntity;
import org.springframework.web.bind.annotation.*;

import java.util.*;

@RestController
@RequestMapping("/users")
public class UserController {

    private final Map<String, User> users = new HashMap<>();

    @GetMapping
    public List<User> list() {
        return new ArrayList<>(users.values());
    }

    @GetMapping("/{id}")
    public ResponseEntity<User> get(@PathVariable String id) {
        User u = users.get(id);
        return u != null ? ResponseEntity.ok(u) : ResponseEntity.notFound().build();
    }

    @PostMapping
    public ResponseEntity<User> create(@RequestBody Map<String, String> body) {
        String id = UUID.randomUUID().toString();
        User u = new User(id, body.get("fullName"), body.get("email"));
        users.put(id, u);
        return ResponseEntity.status(201).body(u);
    }

    @GetMapping("/health")
    public Map<String, String> health() {
        return Map.of("status", "ok");
    }
}
