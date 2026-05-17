package com.ticketing.user;

public class User {
    public String id;
    public String fullName;
    public String email;
    public String role = "AGENT";

    public User() {}
    public User(String id, String fullName, String email) {
        this.id = id; this.fullName = fullName; this.email = email;
    }
}
