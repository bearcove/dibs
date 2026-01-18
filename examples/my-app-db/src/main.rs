//! Database service binary for my-app.
//!
//! Usage:
//!   my-app-db          - Run the dibs service (connects back to CLI via roam)
//!   my-app-db seed     - Seed the database with sample data

use my_app_db::{Category, Comment, Post, PostLike, PostTag, Tag, User, UserFollow};
use tokio_postgres::NoTls;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Touch the types so they're not dead code eliminated
    let _ = (
        std::any::type_name::<User>(),
        std::any::type_name::<Post>(),
        std::any::type_name::<Category>(),
        std::any::type_name::<Tag>(),
        std::any::type_name::<PostTag>(),
        std::any::type_name::<Comment>(),
        std::any::type_name::<PostLike>(),
        std::any::type_name::<UserFollow>(),
    );

    let args: Vec<String> = std::env::args().collect();

    if args.len() > 1 && args[1] == "seed" {
        // Seed needs async runtime
        let rt = tokio::runtime::Runtime::new()?;
        rt.block_on(seed())?;
    } else {
        // run_service creates its own runtime
        dibs::run_service();
    }

    Ok(())
}

async fn seed() -> Result<(), Box<dyn std::error::Error>> {
    let _ = dotenvy::dotenv();

    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://localhost/dibs_test".to_string());

    println!("🌱 Seeding database: {}", database_url);
    println!();

    let (client, connection) = tokio_postgres::connect(&database_url, NoTls).await?;

    // Spawn connection handler
    tokio::spawn(async move {
        if let Err(e) = connection.await {
            eprintln!("Connection error: {}", e);
        }
    });

    // Clear existing data (in reverse FK order to respect constraints)
    println!("🗑️  Clearing existing data...");
    client.execute("DELETE FROM post_likes", &[]).await.ok();
    client.execute("DELETE FROM comments", &[]).await.ok();
    client.execute("DELETE FROM post_tags", &[]).await.ok();
    client.execute("DELETE FROM posts", &[]).await.ok();
    client.execute("DELETE FROM tags", &[]).await.ok();
    client.execute("DELETE FROM categories", &[]).await.ok();
    client.execute("DELETE FROM user_follows", &[]).await.ok();
    client.execute("DELETE FROM users", &[]).await.ok();
    println!();

    // ===== USERS =====
    println!("👥 Creating users...");
    let users = [
        (1i64, "alice@example.com", "Alice Chen", Some("Senior software engineer. Rust enthusiast. Building cool stuff."), Some("https://api.dicebear.com/7.x/avataaars/svg?seed=alice"), true),
        (2i64, "bob@example.com", "Bob Martinez", Some("Full-stack developer. Coffee addict. Open source contributor."), Some("https://api.dicebear.com/7.x/avataaars/svg?seed=bob"), false),
        (3i64, "charlie@example.com", "Charlie Kim", Some("DevOps engineer. Kubernetes wizard. Automation fanatic."), Some("https://api.dicebear.com/7.x/avataaars/svg?seed=charlie"), false),
        (4i64, "diana@example.com", "Diana Patel", Some("Frontend specialist. React & Svelte. Design systems advocate."), Some("https://api.dicebear.com/7.x/avataaars/svg?seed=diana"), false),
        (5i64, "eve@example.com", "Eve Thompson", Some("Security researcher. Bug bounty hunter. CTF player."), Some("https://api.dicebear.com/7.x/avataaars/svg?seed=eve"), true),
        (6i64, "frank@example.com", "Frank Wilson", None::<&str>, None::<&str>, false),
        (7i64, "grace@example.com", "Grace Lee", Some("Data scientist. ML/AI explorer. Python & Rust."), Some("https://api.dicebear.com/7.x/avataaars/svg?seed=grace"), false),
        (8i64, "henry@example.com", "Henry O'Brien", Some("Backend engineer. Database nerd. Performance optimization."), Some("https://api.dicebear.com/7.x/avataaars/svg?seed=henry"), false),
    ];

    for (id, email, name, bio, avatar_url, is_admin) in users {
        client
            .execute(
                "INSERT INTO users (id, email, name, bio, avatar_url, is_admin) VALUES ($1, $2, $3, $4, $5, $6)",
                &[&id, &email, &name, &bio, &avatar_url, &is_admin],
            )
            .await?;
        let admin_badge = if is_admin { " 👑" } else { "" };
        println!("  {} <{}>{}", name, email, admin_badge);
    }
    println!();

    // ===== USER FOLLOWS =====
    println!("🔗 Creating follow relationships...");
    let follows = [
        (2i64, 1i64), // Bob follows Alice
        (3i64, 1i64), // Charlie follows Alice
        (4i64, 1i64), // Diana follows Alice
        (5i64, 1i64), // Eve follows Alice
        (6i64, 1i64), // Frank follows Alice
        (7i64, 1i64), // Grace follows Alice
        (1i64, 5i64), // Alice follows Eve
        (2i64, 5i64), // Bob follows Eve
        (3i64, 2i64), // Charlie follows Bob
        (4i64, 3i64), // Diana follows Charlie
        (7i64, 8i64), // Grace follows Henry
        (8i64, 7i64), // Henry follows Grace
    ];

    for (follower_id, following_id) in follows {
        client
            .execute(
                "INSERT INTO user_follows (follower_id, following_id) VALUES ($1, $2)",
                &[&follower_id, &following_id],
            )
            .await?;
    }
    println!("  Created {} follow relationships", follows.len());
    println!();

    // ===== CATEGORIES =====
    println!("📁 Creating categories...");
    let categories = [
        (1i64, "Programming", "programming", Some("All about writing code"), None::<i64>, 1),
        (2i64, "Rust", "rust", Some("The Rust programming language"), Some(1i64), 1),
        (3i64, "Web Development", "web-dev", Some("Frontend and backend web technologies"), Some(1i64), 2),
        (4i64, "DevOps", "devops", Some("Infrastructure, CI/CD, and operations"), None::<i64>, 2),
        (5i64, "Kubernetes", "kubernetes", Some("Container orchestration with K8s"), Some(4i64), 1),
        (6i64, "Security", "security", Some("Application and infrastructure security"), None::<i64>, 3),
        (7i64, "Tutorials", "tutorials", Some("Step-by-step guides and how-tos"), None::<i64>, 4),
        (8i64, "Opinion", "opinion", Some("Thoughts and perspectives on tech"), None::<i64>, 5),
    ];

    for (id, name, slug, description, parent_id, sort_order) in categories {
        client
            .execute(
                "INSERT INTO categories (id, name, slug, description, parent_id, sort_order) VALUES ($1, $2, $3, $4, $5, $6)",
                &[&id, &name, &slug, &description, &parent_id, &sort_order],
            )
            .await?;
        let indent = if parent_id.is_some() { "    └─ " } else { "  " };
        println!("{}{}", indent, name);
    }
    println!();

    // ===== TAGS =====
    println!("🏷️  Creating tags...");
    let tags = [
        (1i64, "rust", "rust", Some("#DEA584")),
        (2i64, "async", "async", Some("#4B8BBE")),
        (3i64, "performance", "performance", Some("#E74C3C")),
        (4i64, "tutorial", "tutorial", Some("#2ECC71")),
        (5i64, "beginner", "beginner", Some("#9B59B6")),
        (6i64, "advanced", "advanced", Some("#E67E22")),
        (7i64, "web", "web", Some("#3498DB")),
        (8i64, "database", "database", Some("#1ABC9C")),
        (9i64, "security", "security", Some("#C0392B")),
        (10i64, "kubernetes", "kubernetes", Some("#326CE5")),
        (11i64, "docker", "docker", Some("#2496ED")),
        (12i64, "testing", "testing", Some("#F39C12")),
    ];

    for (id, name, slug, color) in tags {
        client
            .execute(
                "INSERT INTO tags (id, name, slug, color) VALUES ($1, $2, $3, $4)",
                &[&id, &name, &slug, &color],
            )
            .await?;
    }
    println!("  Created {} tags", tags.len());
    println!();

    // ===== POSTS =====
    println!("📝 Creating posts...");
    let posts = [
        (1i64, 1i64, Some(2i64), "Getting Started with Rust", "getting-started-with-rust",
         Some("A beginner-friendly introduction to the Rust programming language."),
         "# Getting Started with Rust\n\nRust is a systems programming language that runs blazingly fast, prevents segfaults, and guarantees thread safety.\n\n## Why Rust?\n\n- Memory safety without garbage collection\n- Concurrency without data races\n- Zero-cost abstractions\n\n## Your First Program\n\n```rust\nfn main() {\n    println!(\"Hello, world!\");\n}\n```\n\nLet's dive in!",
         true, 1523),
        (2i64, 1i64, Some(2i64), "Async Rust: A Deep Dive", "async-rust-deep-dive",
         Some("Understanding async/await, futures, and the tokio runtime."),
         "# Async Rust: A Deep Dive\n\nAsync programming in Rust is powerful but can be confusing at first. Let's break it down.\n\n## The Basics\n\nAsync functions return a `Future` that must be awaited...",
         true, 892),
        (3i64, 1i64, Some(2i64), "Zero-Cost Abstractions in Rust", "zero-cost-abstractions",
         Some("How Rust achieves high-level ergonomics with low-level performance."),
         "# Zero-Cost Abstractions\n\nOne of Rust's core principles is that abstractions should have no runtime cost...",
         true, 654),
        (4i64, 2i64, Some(3i64), "Building Modern Web Apps with Rust", "modern-web-apps-rust",
         Some("A guide to building full-stack web applications using Rust."),
         "# Building Modern Web Apps with Rust\n\nRust isn't just for systems programming - it's great for web development too!\n\n## The Stack\n\n- Axum for the backend\n- SQLx for database access\n- HTMX for interactivity\n\nLet's build something awesome.",
         true, 2341),
        (5i64, 2i64, Some(3i64), "HTMX + Rust: A Perfect Match", "htmx-rust-perfect-match",
         Some("Why HTMX and Rust backends work so well together."),
         "# HTMX + Rust: A Perfect Match\n\nForget complex JavaScript frameworks. HTMX lets you build dynamic UIs with simple HTML attributes...",
         true, 1876),
        (6i64, 3i64, Some(5i64), "Kubernetes for Developers", "kubernetes-for-developers",
         Some("A practical introduction to Kubernetes from a developer's perspective."),
         "# Kubernetes for Developers\n\nYou don't need to be a DevOps expert to understand Kubernetes. Here's what developers need to know...",
         true, 3102),
        (7i64, 3i64, Some(4i64), "GitOps with ArgoCD", "gitops-argocd",
         Some("Implementing GitOps workflows using ArgoCD."),
         "# GitOps with ArgoCD\n\nGitOps is a way of implementing continuous deployment for cloud native applications...",
         true, 987),
        (8i64, 5i64, Some(6i64), "Web Security Fundamentals", "web-security-fundamentals",
         Some("Essential security concepts every developer should know."),
         "# Web Security Fundamentals\n\nSecurity isn't just for security teams. Every developer should understand these basics...\n\n## OWASP Top 10\n\n1. Injection\n2. Broken Authentication\n3. ...",
         true, 4521),
        (9i64, 5i64, Some(6i64), "Rust Memory Safety Deep Dive", "rust-memory-safety",
         Some("How Rust prevents common memory vulnerabilities."),
         "# Rust Memory Safety Deep Dive\n\nLet's explore how Rust's ownership system prevents buffer overflows, use-after-free, and other memory bugs...",
         true, 2134),
        (10i64, 4i64, Some(3i64), "CSS Grid Mastery", "css-grid-mastery",
         Some("Everything you need to know about CSS Grid layout."),
         "# CSS Grid Mastery\n\nCSS Grid has revolutionized web layout. Here's how to use it effectively...",
         true, 1654),
        (11i64, 7i64, Some(8i64), "The Future of AI in Development", "future-ai-development",
         Some("How AI tools are changing the way we write code."),
         "# The Future of AI in Development\n\nAI coding assistants are here. What does this mean for developers?",
         true, 5432),
        (12i64, 8i64, Some(8i64), "PostgreSQL Performance Tips", "postgresql-performance-tips",
         Some("Optimize your PostgreSQL queries and configuration."),
         "# PostgreSQL Performance Tips\n\nDatabase performance can make or break your application. Here are my top tips...\n\n## Indexing Strategies\n\n## Query Optimization\n\n## Configuration Tuning",
         true, 2876),
        (13i64, 1i64, Some(2i64), "Draft: Rust 2024 Edition Preview", "rust-2024-edition-preview",
         None::<&str>,
         "# Rust 2024 Edition Preview\n\n[DRAFT - Work in progress]\n\nNotes on what's coming in the next Rust edition...",
         false, 0),
        (14i64, 2i64, Some(7i64), "Draft: WebAssembly Tutorial", "wasm-tutorial-draft",
         None::<&str>,
         "# WebAssembly with Rust\n\n[DRAFT]\n\nOutline:\n- What is WASM?\n- Setting up\n- Building your first module",
         false, 0),
    ];

    for (id, author_id, category_id, title, slug, excerpt, body, published, view_count) in posts {
        client
            .execute(
                "INSERT INTO posts (id, author_id, category_id, title, slug, excerpt, body, published, view_count, published_at)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, CASE WHEN $8 THEN now() - interval '1 day' * (14 - $1) ELSE NULL END)",
                &[&id, &author_id, &category_id, &title, &slug, &excerpt, &body, &published, &view_count],
            )
            .await?;
        let status = if published { "✓" } else { "📝" };
        println!("  {} \"{}\" (views: {})", status, title, view_count);
    }
    println!();

    // ===== POST TAGS =====
    println!("🔖 Tagging posts...");
    let post_tags = [
        (1i64, 1i64), (1, 4), (1, 5),           // Post 1: rust, tutorial, beginner
        (2i64, 1i64), (2, 2), (2, 6),           // Post 2: rust, async, advanced
        (3i64, 1i64), (3, 3), (3, 6),           // Post 3: rust, performance, advanced
        (4i64, 1i64), (4, 7), (4, 8),           // Post 4: rust, web, database
        (5i64, 7i64), (5, 1),                   // Post 5: web, rust
        (6i64, 10i64), (6, 11), (6, 4),         // Post 6: kubernetes, docker, tutorial
        (7i64, 10i64), (7, 11),                 // Post 7: kubernetes, docker
        (8i64, 9i64), (8, 7), (8, 4),           // Post 8: security, web, tutorial
        (9i64, 9i64), (9, 1), (9, 6),           // Post 9: security, rust, advanced
        (10i64, 7i64), (10, 4),                 // Post 10: web, tutorial
        (11i64, 6i64),                          // Post 11: advanced
        (12i64, 8i64), (12, 3),                 // Post 12: database, performance
    ];

    for (post_id, tag_id) in post_tags {
        client
            .execute(
                "INSERT INTO post_tags (post_id, tag_id) VALUES ($1, $2)",
                &[&post_id, &tag_id],
            )
            .await?;
    }
    println!("  Created {} post-tag associations", post_tags.len());
    println!();

    // ===== COMMENTS =====
    println!("💬 Creating comments...");
    let comments = [
        (1i64, 1i64, 2i64, None::<i64>, "Great introduction! This helped me finally understand ownership."),
        (2i64, 1i64, 4i64, None::<i64>, "Clear and concise. Would love to see a follow-up on lifetimes!"),
        (3i64, 1i64, 1i64, Some(2i64), "Thanks Diana! Lifetimes article is in the works 😊"),
        (4i64, 2i64, 3i64, None::<i64>, "The tokio examples were super helpful. Bookmarked!"),
        (5i64, 2i64, 7i64, None::<i64>, "Can you do a comparison with async-std?"),
        (6i64, 4i64, 8i64, None::<i64>, "This convinced me to try Rust for my next web project."),
        (7i64, 4i64, 3i64, Some(6i64), "Do it! The ecosystem has matured a lot recently."),
        (8i64, 6i64, 4i64, None::<i64>, "Finally a K8s tutorial that doesn't assume I'm already a DevOps expert!"),
        (9i64, 6i64, 2i64, None::<i64>, "The diagrams really helped visualize the concepts."),
        (10i64, 8i64, 1i64, None::<i64>, "Good overview of the OWASP top 10. Security should be taught more."),
        (11i64, 8i64, 5i64, Some(10i64), "Agreed. Too many devs treat security as an afterthought."),
        (12i64, 11i64, 6i64, None::<i64>, "Hot take: AI will never replace the need to understand fundamentals."),
        (13i64, 11i64, 7i64, Some(12i64), "True, but it's a great learning accelerator!"),
        (14i64, 12i64, 8i64, None::<i64>, "The index optimization tips saved us hours of debugging. Thanks!"),
    ];

    for (id, post_id, author_id, parent_id, body) in comments {
        client
            .execute(
                "INSERT INTO comments (id, post_id, author_id, parent_id, body) VALUES ($1, $2, $3, $4, $5)",
                &[&id, &post_id, &author_id, &parent_id, &body],
            )
            .await?;
    }
    println!("  Created {} comments ({} replies)", comments.len(), comments.iter().filter(|c| c.3.is_some()).count());
    println!();

    // ===== POST LIKES =====
    println!("❤️  Creating likes...");
    let likes = [
        (2i64, 1i64), (3, 1), (4, 1), (5, 1), (6, 1), (7, 1),  // Post 1 popular
        (1i64, 2i64), (3, 2), (7, 2),                          // Post 2
        (2i64, 4i64), (4, 4), (6, 4), (8, 4),                  // Post 4
        (1i64, 6i64), (2, 6), (4, 6), (5, 6), (7, 6),          // Post 6 popular
        (1i64, 8i64), (2, 8), (3, 8), (4, 8), (6, 8), (7, 8), (8, 8),  // Post 8 very popular
        (3i64, 11i64), (4, 11), (5, 11), (6, 11), (7, 11), (8, 11),    // Post 11 AI article popular
        (1i64, 12i64), (3, 12), (7, 12),                        // Post 12
    ];

    for (user_id, post_id) in likes {
        client
            .execute(
                "INSERT INTO post_likes (user_id, post_id) VALUES ($1, $2)",
                &[&user_id, &post_id],
            )
            .await?;
    }
    println!("  Created {} likes", likes.len());
    println!();

    // ===== SUMMARY =====
    println!("═══════════════════════════════════════");
    println!("🎉 Seeding complete!");
    println!("═══════════════════════════════════════");
    println!("  👥 {} users (2 admins)", users.len());
    println!("  🔗 {} follow relationships", follows.len());
    println!("  📁 {} categories", categories.len());
    println!("  🏷️  {} tags", tags.len());
    println!("  📝 {} posts ({} published, {} drafts)",
             posts.len(),
             posts.iter().filter(|p| p.7).count(),
             posts.iter().filter(|p| !p.7).count());
    println!("  🔖 {} post-tag associations", post_tags.len());
    println!("  💬 {} comments", comments.len());
    println!("  ❤️  {} likes", likes.len());
    println!("═══════════════════════════════════════");

    Ok(())
}
