/// commands/hep.rs — Home Exercise Program (HEP) Builder (M003/S02)
///
/// Implements the HEP Builder feature set:
///   HEP-01  Exercise library — ~50 common PT exercises, filterable by body region and category.
///   HEP-02  Program builder — create/update a HEP program for a patient encounter.
///   HEP-03  Templates — save a program as a reusable template; built-in templates included.
///
/// Data model
/// ----------
/// Exercise library:  `exercise_library` (seeded once on first run).
/// Programs:          `hep_programs` — one row per program; exercises serialized as JSON.
/// Templates:         `hep_templates` — one row per template; exercises with defaults as JSON.
///
/// RBAC
/// ----
/// All HEP commands require `ClinicalDocumentation` resource access.
///   Provider / SystemAdmin  → full CRUD
///   NurseMa                 → Create + Read + Update
///   BillingStaff / FrontDesk → Read-only (no create/update)
///
/// Audit
/// -----
/// Every command writes an audit row using `write_audit_entry`.
/// Audit action strings: hep.list_exercises, hep.search_exercises, hep.create_program,
///                       hep.get_program, hep.list_programs, hep.update_program,
///                       hep.create_template, hep.list_templates, hep.get_template
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::audit::{write_audit_entry, AuditEntryInput};
use crate::auth::session::SessionManager;
use crate::db::connection::Database;
use crate::device_id::DeviceId;
use crate::error::AppError;
use crate::rbac::middleware;
use crate::rbac::roles::{Action, Resource};

// ─────────────────────────────────────────────────────────────────────────────
// Domain types
// ─────────────────────────────────────────────────────────────────────────────

/// A single exercise in the library.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Exercise {
    pub exercise_id: String,
    pub name: String,
    pub body_region: String,
    pub category: String,
    pub description: Option<String>,
    pub instructions: Option<String>,
    pub equipment: Option<String>,
    pub difficulty: Option<String>,
    pub is_builtin: bool,
}

/// Prescription details for a single exercise within a HEP program.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExercisePrescription {
    pub exercise_id: String,
    pub sets: Option<u32>,
    pub reps: Option<u32>,
    pub duration_seconds: Option<u32>,
    pub hold_seconds: Option<u32>,
    pub times_per_day: Option<u32>,
    pub days_per_week: Option<u32>,
    pub resistance: Option<String>,
    pub pain_limit: Option<u32>,
    pub notes: Option<String>,
}

/// Input for creating a new HEP program.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateHepProgramInput {
    pub patient_id: String,
    pub encounter_id: Option<String>,
    pub exercises: Vec<ExercisePrescription>,
    pub notes: Option<String>,
}

/// A saved HEP program record.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HepProgram {
    pub program_id: String,
    pub patient_id: String,
    pub encounter_id: Option<String>,
    pub created_by: String,
    pub exercises: Vec<ExercisePrescription>,
    pub notes: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// Input for saving a HEP template.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateHepTemplateInput {
    pub name: String,
    pub body_region: Option<String>,
    pub condition_name: Option<String>,
    pub exercises: Vec<ExercisePrescription>,
}

/// A saved HEP template record.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HepTemplate {
    pub template_id: String,
    pub name: String,
    pub body_region: Option<String>,
    pub condition_name: Option<String>,
    pub exercises: Vec<ExercisePrescription>,
    pub created_by: String,
    pub is_builtin: bool,
    pub created_at: String,
}

// ─────────────────────────────────────────────────────────────────────────────
// Exercise library seed data
// ─────────────────────────────────────────────────────────────────────────────

struct ExerciseSeed {
    id: &'static str,
    name: &'static str,
    body_region: &'static str,
    category: &'static str,
    description: &'static str,
    instructions: &'static str,
    equipment: &'static str,
    difficulty: &'static str,
}

const EXERCISE_SEEDS: &[ExerciseSeed] = &[
    // ── Cervical ──────────────────────────────────────────────────────────────
    ExerciseSeed {
        id: "ex-cerv-01",
        name: "Cervical Chin Tucks",
        body_region: "cervical",
        category: "rom",
        description: "Gently retract the chin to restore cervical alignment.",
        instructions: "Sit or stand tall. Gently draw your chin straight back. Hold, then release.",
        equipment: "",
        difficulty: "beginner",
    },
    ExerciseSeed {
        id: "ex-cerv-02",
        name: "Cervical Side Bending Stretch",
        body_region: "cervical",
        category: "stretching",
        description: "Lateral flexion stretch for cervical musculature.",
        instructions: "Tilt your ear toward your shoulder. Use hand to gently deepen stretch. Hold.",
        equipment: "",
        difficulty: "beginner",
    },
    ExerciseSeed {
        id: "ex-cerv-03",
        name: "Cervical Rotation ROM",
        body_region: "cervical",
        category: "rom",
        description: "Active cervical rotation to restore range of motion.",
        instructions: "Slowly rotate head to one side until a gentle stretch is felt. Return to center.",
        equipment: "",
        difficulty: "beginner",
    },
    ExerciseSeed {
        id: "ex-cerv-04",
        name: "Cervical Isometric Strengthening",
        body_region: "cervical",
        category: "strengthening",
        description: "Isometric resistance to build cervical stabilizer strength.",
        instructions: "Place palm on forehead. Push forehead into hand while resisting with hand. Hold.",
        equipment: "",
        difficulty: "intermediate",
    },
    ExerciseSeed {
        id: "ex-cerv-05",
        name: "Scapular Retraction",
        body_region: "cervical",
        category: "strengthening",
        description: "Strengthen periscapular muscles to offload cervical spine.",
        instructions: "Squeeze shoulder blades together and hold. Slowly release.",
        equipment: "",
        difficulty: "beginner",
    },
    // ── Thoracic ──────────────────────────────────────────────────────────────
    ExerciseSeed {
        id: "ex-thor-01",
        name: "Thoracic Extension over Foam Roll",
        body_region: "thoracic",
        category: "rom",
        description: "Restore thoracic extension mobility using a foam roller.",
        instructions: "Place foam roll perpendicular to spine at mid-back. Support head. Gently extend over the roll.",
        equipment: "foam roller",
        difficulty: "beginner",
    },
    ExerciseSeed {
        id: "ex-thor-02",
        name: "Thoracic Rotation Stretch (Seated)",
        body_region: "thoracic",
        category: "stretching",
        description: "Seated rotation to improve thoracic mobility.",
        instructions: "Sit in chair, cross arms on chest. Rotate torso slowly left then right.",
        equipment: "chair",
        difficulty: "beginner",
    },
    ExerciseSeed {
        id: "ex-thor-03",
        name: "Cat-Cow",
        body_region: "thoracic",
        category: "rom",
        description: "Spinal flexion/extension for thoracic and lumbar mobility.",
        instructions: "On hands and knees, arch back up (cat) then drop belly (cow). Breathe throughout.",
        equipment: "mat",
        difficulty: "beginner",
    },
    // ── Lumbar ────────────────────────────────────────────────────────────────
    ExerciseSeed {
        id: "ex-lumb-01",
        name: "Lumbar Pelvic Tilts",
        body_region: "lumbar",
        category: "rom",
        description: "Gentle lumbar mobilization and core activation.",
        instructions: "Lie on back, knees bent. Flatten lower back to floor by tightening abs. Hold then release.",
        equipment: "mat",
        difficulty: "beginner",
    },
    ExerciseSeed {
        id: "ex-lumb-02",
        name: "Knee-to-Chest Stretch",
        body_region: "lumbar",
        category: "stretching",
        description: "Single or double knee to chest for lumbar decompression.",
        instructions: "Lie on back. Pull one or both knees gently to chest. Hold.",
        equipment: "mat",
        difficulty: "beginner",
    },
    ExerciseSeed {
        id: "ex-lumb-03",
        name: "Prone Press-Up (McKenzie Extension)",
        body_region: "lumbar",
        category: "rom",
        description: "McKenzie extension to centralize disc symptoms.",
        instructions: "Lie prone. Place hands under shoulders. Press torso up, hips stay on mat.",
        equipment: "mat",
        difficulty: "beginner",
    },
    ExerciseSeed {
        id: "ex-lumb-04",
        name: "Dead Bug",
        body_region: "lumbar",
        category: "strengthening",
        description: "Core stabilization exercise for lumbar spine.",
        instructions: "Lie on back, arms up, knees at 90°. Lower opposite arm and leg simultaneously while maintaining neutral spine.",
        equipment: "mat",
        difficulty: "intermediate",
    },
    ExerciseSeed {
        id: "ex-lumb-05",
        name: "Bird Dog",
        body_region: "lumbar",
        category: "strengthening",
        description: "Multi-planar core stability for lumbar support.",
        instructions: "On hands and knees. Extend opposite arm and leg. Hold 3–5 s. Return and repeat.",
        equipment: "mat",
        difficulty: "intermediate",
    },
    ExerciseSeed {
        id: "ex-lumb-06",
        name: "Glute Bridges",
        body_region: "lumbar",
        category: "strengthening",
        description: "Posterior chain strengthening to support lumbar spine.",
        instructions: "Lie on back, knees bent. Drive hips to ceiling, squeezing glutes. Hold then lower.",
        equipment: "mat",
        difficulty: "beginner",
    },
    ExerciseSeed {
        id: "ex-lumb-07",
        name: "Side-Lying Clam Shell",
        body_region: "lumbar",
        category: "strengthening",
        description: "Hip abductor strengthening for lumbar/SI stability.",
        instructions: "Lie on side, knees bent. Rotate top knee upward like opening a clam. Hold then lower.",
        equipment: "mat",
        difficulty: "beginner",
    },
    // ── Shoulder ──────────────────────────────────────────────────────────────
    ExerciseSeed {
        id: "ex-shldr-01",
        name: "Pendulum (Codman) Exercise",
        body_region: "shoulder",
        category: "rom",
        description: "Gravity-assisted shoulder distraction and gentle ROM.",
        instructions: "Lean forward, let arm hang freely. Swing arm in small circles using body momentum.",
        equipment: "",
        difficulty: "beginner",
    },
    ExerciseSeed {
        id: "ex-shldr-02",
        name: "Shoulder Pulley ROM",
        body_region: "shoulder",
        category: "rom",
        description: "Assistive shoulder elevation using overhead pulley.",
        instructions: "Sit under pulley. Use uninvolved arm to lift involved arm overhead.",
        equipment: "overhead pulley",
        difficulty: "beginner",
    },
    ExerciseSeed {
        id: "ex-shldr-03",
        name: "External Rotation with Resistive Band",
        body_region: "shoulder",
        category: "strengthening",
        description: "Rotator cuff external rotation strengthening.",
        instructions: "Elbow at 90° at side. Pull band outward rotating forearm away from body.",
        equipment: "resistance band",
        difficulty: "intermediate",
    },
    ExerciseSeed {
        id: "ex-shldr-04",
        name: "Internal Rotation with Resistive Band",
        body_region: "shoulder",
        category: "strengthening",
        description: "Rotator cuff internal rotation strengthening.",
        instructions: "Elbow at 90° at side. Pull band inward rotating forearm toward body.",
        equipment: "resistance band",
        difficulty: "intermediate",
    },
    ExerciseSeed {
        id: "ex-shldr-05",
        name: "Shoulder Scaption",
        body_region: "shoulder",
        category: "strengthening",
        description: "Supraspinatus strengthening in scapular plane.",
        instructions: "Stand, arm 30° forward of coronal plane. Raise arm to shoulder height with thumb up.",
        equipment: "dumbbell",
        difficulty: "intermediate",
    },
    ExerciseSeed {
        id: "ex-shldr-06",
        name: "Cross Body Shoulder Stretch",
        body_region: "shoulder",
        category: "stretching",
        description: "Posterior capsule stretch for shoulder horizontal adduction.",
        instructions: "Pull arm horizontally across chest with opposite hand. Hold.",
        equipment: "",
        difficulty: "beginner",
    },
    ExerciseSeed {
        id: "ex-shldr-07",
        name: "Sleeper Stretch",
        body_region: "shoulder",
        category: "stretching",
        description: "Posterior shoulder capsule stretch in side-lying.",
        instructions: "Lie on affected side, arm at 90°. Use other hand to gently push forearm toward floor.",
        equipment: "mat",
        difficulty: "beginner",
    },
    // ── Elbow ─────────────────────────────────────────────────────────────────
    ExerciseSeed {
        id: "ex-elbow-01",
        name: "Wrist Flexor Stretch (Tennis Elbow)",
        body_region: "elbow",
        category: "stretching",
        description: "Stretch wrist extensors to address lateral epicondylalgia.",
        instructions: "Extend arm, palm down. Use other hand to gently bend wrist downward. Hold.",
        equipment: "",
        difficulty: "beginner",
    },
    ExerciseSeed {
        id: "ex-elbow-02",
        name: "Eccentric Wrist Extension (Tyler Twist)",
        body_region: "elbow",
        category: "strengthening",
        description: "Eccentric loading for lateral epicondylalgia rehab.",
        instructions: "Hold bar or flex bar. Use both hands to extend wrist, then lower slowly with affected hand only.",
        equipment: "flex bar",
        difficulty: "intermediate",
    },
    ExerciseSeed {
        id: "ex-elbow-03",
        name: "Forearm Pronation/Supination",
        body_region: "elbow",
        category: "rom",
        description: "Restore forearm rotation ROM.",
        instructions: "Elbow at 90°. Rotate forearm palm-up then palm-down through available range.",
        equipment: "",
        difficulty: "beginner",
    },
    // ── Wrist ─────────────────────────────────────────────────────────────────
    ExerciseSeed {
        id: "ex-wrist-01",
        name: "Wrist Flexion/Extension ROM",
        body_region: "wrist",
        category: "rom",
        description: "Active wrist flexion and extension to restore range.",
        instructions: "Support forearm. Actively flex then extend wrist through full available range.",
        equipment: "",
        difficulty: "beginner",
    },
    ExerciseSeed {
        id: "ex-wrist-02",
        name: "Wrist Radial/Ulnar Deviation",
        body_region: "wrist",
        category: "rom",
        description: "Active wrist deviation to restore lateral mobility.",
        instructions: "Forearm supported, thumb up. Move wrist toward thumb (radial) then pinky (ulnar).",
        equipment: "",
        difficulty: "beginner",
    },
    ExerciseSeed {
        id: "ex-wrist-03",
        name: "Grip Strengthening (Putty/Ball)",
        body_region: "wrist",
        category: "strengthening",
        description: "Grip strength training with putty or squeeze ball.",
        instructions: "Squeeze putty or ball firmly, hold, then slowly release.",
        equipment: "therapy putty",
        difficulty: "beginner",
    },
    // ── Hip ───────────────────────────────────────────────────────────────────
    ExerciseSeed {
        id: "ex-hip-01",
        name: "Hip Flexor Stretch (Kneeling Lunge)",
        body_region: "hip",
        category: "stretching",
        description: "Kneeling hip flexor stretch for iliopsoas.",
        instructions: "Half kneeling position. Shift pelvis forward until stretch felt in front of rear hip.",
        equipment: "mat",
        difficulty: "beginner",
    },
    ExerciseSeed {
        id: "ex-hip-02",
        name: "Piriformis Stretch (Figure 4)",
        body_region: "hip",
        category: "stretching",
        description: "Deep external rotator and piriformis stretch.",
        instructions: "Lie on back. Cross ankle over opposite knee. Pull thigh toward chest until stretch felt.",
        equipment: "mat",
        difficulty: "beginner",
    },
    ExerciseSeed {
        id: "ex-hip-03",
        name: "Standing Hip Abduction",
        body_region: "hip",
        category: "strengthening",
        description: "Hip abductor strengthening for lateral stability.",
        instructions: "Stand on one leg. Lift opposite leg out to side. Slowly lower.",
        equipment: "resistance band",
        difficulty: "beginner",
    },
    ExerciseSeed {
        id: "ex-hip-04",
        name: "Side-Lying Hip Abduction",
        body_region: "hip",
        category: "strengthening",
        description: "Gluteus medius strengthening in side-lying.",
        instructions: "Lie on side, legs straight. Lift top leg to 45°. Hold then lower slowly.",
        equipment: "mat",
        difficulty: "beginner",
    },
    ExerciseSeed {
        id: "ex-hip-05",
        name: "Hip Extension (Prone)",
        body_region: "hip",
        category: "strengthening",
        description: "Prone hip extension for gluteus maximus.",
        instructions: "Lie prone, tighten abs. Lift one leg off mat keeping knee straight. Hold.",
        equipment: "mat",
        difficulty: "beginner",
    },
    // ── Knee ──────────────────────────────────────────────────────────────────
    ExerciseSeed {
        id: "ex-knee-01",
        name: "Quad Sets",
        body_region: "knee",
        category: "strengthening",
        description: "Isometric quadriceps activation, ideal post-op.",
        instructions: "Sit or lie with leg extended. Tighten quad to press back of knee into surface. Hold.",
        equipment: "mat",
        difficulty: "beginner",
    },
    ExerciseSeed {
        id: "ex-knee-02",
        name: "Straight Leg Raise (SLR)",
        body_region: "knee",
        category: "strengthening",
        description: "Quadriceps strengthening without knee stress.",
        instructions: "Tighten quad on straight leg. Lift to height of opposite bent knee. Hold. Lower slowly.",
        equipment: "mat",
        difficulty: "beginner",
    },
    ExerciseSeed {
        id: "ex-knee-03",
        name: "Terminal Knee Extension (TKE)",
        body_region: "knee",
        category: "strengthening",
        description: "End-range quadriceps strengthening with band.",
        instructions: "Stand with band behind knee. Start slightly bent. Straighten knee against band resistance.",
        equipment: "resistance band",
        difficulty: "beginner",
    },
    ExerciseSeed {
        id: "ex-knee-04",
        name: "Knee Flexion ROM (Heel Slides)",
        body_region: "knee",
        category: "rom",
        description: "Active knee flexion ROM exercise for post-op recovery.",
        instructions: "Lie on back. Slide heel toward buttocks, bending knee as far as tolerated. Slowly extend.",
        equipment: "mat",
        difficulty: "beginner",
    },
    ExerciseSeed {
        id: "ex-knee-05",
        name: "Short Arc Quad (SAQ)",
        body_region: "knee",
        category: "strengthening",
        description: "Quadriceps strengthening in 0–30° arc.",
        instructions: "Place bolster under knee (30° flexion). Extend knee fully. Hold. Lower slowly.",
        equipment: "bolster",
        difficulty: "beginner",
    },
    ExerciseSeed {
        id: "ex-knee-06",
        name: "Wall Slides / Wall Squats",
        body_region: "knee",
        category: "strengthening",
        description: "Closed-chain quad and glute strengthening.",
        instructions: "Back against wall. Slide down to 60–90° knee flexion. Hold. Slide back up.",
        equipment: "",
        difficulty: "intermediate",
    },
    ExerciseSeed {
        id: "ex-knee-07",
        name: "Hamstring Curls (Standing with Band)",
        body_region: "knee",
        category: "strengthening",
        description: "Hamstring strengthening for dynamic knee stability.",
        instructions: "Band around ankle, anchored at front. Curl heel toward buttock against resistance.",
        equipment: "resistance band",
        difficulty: "intermediate",
    },
    // ── Ankle ─────────────────────────────────────────────────────────────────
    ExerciseSeed {
        id: "ex-ankl-01",
        name: "Ankle Alphabet",
        body_region: "ankle",
        category: "rom",
        description: "Multi-directional ankle ROM through alphabet tracing.",
        instructions: "Seated, foot elevated. Trace alphabet in air with big toe.",
        equipment: "",
        difficulty: "beginner",
    },
    ExerciseSeed {
        id: "ex-ankl-02",
        name: "Calf Raises (Gastrocnemius)",
        body_region: "ankle",
        category: "strengthening",
        description: "Gastrocnemius and soleus strengthening.",
        instructions: "Stand on edge of step, heels hanging. Rise onto toes. Lower slowly.",
        equipment: "step",
        difficulty: "beginner",
    },
    ExerciseSeed {
        id: "ex-ankl-03",
        name: "Theraband Ankle Dorsiflexion",
        body_region: "ankle",
        category: "strengthening",
        description: "Tibialis anterior strengthening for dorsiflexion deficit.",
        instructions: "Seated, band around top of foot anchored at base. Pull toes up against band.",
        equipment: "resistance band",
        difficulty: "beginner",
    },
    ExerciseSeed {
        id: "ex-ankl-04",
        name: "Theraband Ankle Inversion",
        body_region: "ankle",
        category: "strengthening",
        description: "Peroneals and ankle invertors strengthening.",
        instructions: "Seated, band around foot. Move foot inward against band resistance.",
        equipment: "resistance band",
        difficulty: "beginner",
    },
    ExerciseSeed {
        id: "ex-ankl-05",
        name: "Single Leg Balance",
        body_region: "ankle",
        category: "balance",
        description: "Proprioceptive training for ankle stability.",
        instructions: "Stand on one foot, slight knee bend. Hold balance for time. Progress to eyes closed or unstable surface.",
        equipment: "",
        difficulty: "beginner",
    },
    ExerciseSeed {
        id: "ex-ankl-06",
        name: "Bosu Ball Single Leg Stance",
        body_region: "ankle",
        category: "balance",
        description: "Advanced proprioceptive training on unstable surface.",
        instructions: "Stand on BOSU flat side up. Balance on one foot for timed intervals.",
        equipment: "bosu ball",
        difficulty: "advanced",
    },
    // ── General ───────────────────────────────────────────────────────────────
    ExerciseSeed {
        id: "ex-gen-01",
        name: "Stationary Bike (Warm-Up)",
        body_region: "general",
        category: "cardio",
        description: "Low-impact cardiovascular warm-up for all conditions.",
        instructions: "Adjust seat height. Pedal at comfortable resistance for prescribed duration.",
        equipment: "stationary bike",
        difficulty: "beginner",
    },
    ExerciseSeed {
        id: "ex-gen-02",
        name: "Treadmill Walking",
        body_region: "general",
        category: "cardio",
        description: "Functional gait training and cardiovascular conditioning.",
        instructions: "Set treadmill to comfortable speed. Walk with upright posture for prescribed time.",
        equipment: "treadmill",
        difficulty: "beginner",
    },
    ExerciseSeed {
        id: "ex-gen-03",
        name: "Seated Marching",
        body_region: "general",
        category: "functional",
        description: "Low-level hip flexor and core activation for deconditioned patients.",
        instructions: "Sit in chair with good posture. Alternate lifting knees to hip height.",
        equipment: "chair",
        difficulty: "beginner",
    },
    ExerciseSeed {
        id: "ex-gen-04",
        name: "Standing March",
        body_region: "general",
        category: "balance",
        description: "Single-leg balance and hip flexor activation for dynamic stability.",
        instructions: "Stand tall, hands on surface for support if needed. Lift one knee to hip height, hold 1–2 s, alternate.",
        equipment: "",
        difficulty: "beginner",
    },
];

/// Seed the exercise library if it has not yet been populated.
/// Called at program creation time; idempotent (uses INSERT OR IGNORE).
fn seed_exercise_library(conn: &rusqlite::Connection) -> Result<(), AppError> {
    for ex in EXERCISE_SEEDS {
        conn.execute(
            "INSERT OR IGNORE INTO exercise_library
                (exercise_id, name, body_region, category, description, instructions, equipment, difficulty, is_builtin)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 1)",
            rusqlite::params![
                ex.id,
                ex.name,
                ex.body_region,
                ex.category,
                ex.description,
                ex.instructions,
                ex.equipment,
                ex.difficulty,
            ],
        )
        .map_err(|e| AppError::Database(e.to_string()))?;
    }
    Ok(())
}

/// Seed built-in HEP templates. Idempotent (uses INSERT OR IGNORE).
fn seed_builtin_templates(conn: &rusqlite::Connection) -> Result<(), AppError> {
    let templates: &[(&str, &str, &str, &str, &[(&str, u32, u32)])] = &[
        (
            "tmpl-low-back-stab",
            "Low Back Stabilization",
            "lumbar",
            "Lumbar instability / low back pain",
            &[
                ("ex-lumb-01", 3, 10), // Pelvic Tilts
                ("ex-lumb-04", 3, 10), // Dead Bug
                ("ex-lumb-05", 3, 10), // Bird Dog
                ("ex-lumb-06", 3, 15), // Glute Bridges
                ("ex-lumb-02", 3, 30), // Knee-to-Chest Stretch (hold seconds)
            ],
        ),
        (
            "tmpl-shldr-rom",
            "Shoulder ROM Recovery",
            "shoulder",
            "Post-operative shoulder / frozen shoulder",
            &[
                ("ex-shldr-01", 1, 30), // Pendulums
                ("ex-shldr-02", 3, 10), // Pulley ROM
                ("ex-shldr-06", 3, 30), // Cross Body Stretch
                ("ex-shldr-07", 3, 30), // Sleeper Stretch
                ("ex-shldr-03", 3, 15), // ER Band
            ],
        ),
        (
            "tmpl-knee-postop-1",
            "Knee Post-Op Phase 1",
            "knee",
            "Post-operative knee (TKR / ACL)",
            &[
                ("ex-knee-01", 3, 10), // Quad Sets
                ("ex-knee-02", 3, 10), // SLR
                ("ex-knee-04", 3, 10), // Heel Slides
                ("ex-knee-05", 3, 10), // SAQ
                ("ex-ankl-01", 1, 1),  // Ankle Alphabet
            ],
        ),
        (
            "tmpl-general-cond",
            "General Conditioning",
            "general",
            "General deconditioning / post-hospitalization",
            &[
                ("ex-gen-03", 3, 10), // Seated Marching
                ("ex-gen-01", 1, 1),  // Stationary Bike
                ("ex-lumb-06", 3, 10), // Glute Bridges
                ("ex-hip-04", 3, 10), // Side-Lying Abduction
                ("ex-ankl-02", 3, 10), // Calf Raises
            ],
        ),
    ];

    let now = chrono::Utc::now().to_rfc3339();

    for (id, name, region, condition, exercises) in templates {
        // Build exercises JSON with default prescriptions
        let prescriptions: Vec<ExercisePrescription> = exercises
            .iter()
            .map(|(ex_id, sets, reps)| ExercisePrescription {
                exercise_id: ex_id.to_string(),
                sets: Some(*sets),
                reps: Some(*reps),
                duration_seconds: None,
                hold_seconds: None,
                times_per_day: Some(1),
                days_per_week: Some(7),
                resistance: None,
                pain_limit: Some(4),
                notes: None,
            })
            .collect();

        let exercises_json = serde_json::to_string(&prescriptions)
            .map_err(|e| AppError::Serialization(e.to_string()))?;

        conn.execute(
            "INSERT OR IGNORE INTO hep_templates
                (template_id, name, body_region, condition_name, exercises_json, created_by, is_builtin, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, 'system', 1, ?6)",
            rusqlite::params![id, name, region, condition, exercises_json, now],
        )
        .map_err(|e| AppError::Database(e.to_string()))?;
    }

    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────────
// Tauri commands
// ─────────────────────────────────────────────────────────────────────────────

/// List exercises from the library, optionally filtered by body region and/or category.
///
/// Requires: ClinicalDocumentation + Read
#[tauri::command]
pub async fn list_exercises(
    body_region: Option<String>,
    category: Option<String>,
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
    device_id: State<'_, DeviceId>,
) -> Result<Vec<Exercise>, AppError> {
    let sess = middleware::require_authenticated(&session)?;
    middleware::require_permission(sess.role, Resource::ClinicalDocumentation, Action::Read)?;

    let conn = db.conn.lock().map_err(|e| AppError::Database(e.to_string()))?;
    seed_exercise_library(&conn)?;

    let mut query = String::from(
        "SELECT exercise_id, name, body_region, category, description, instructions, equipment, difficulty, is_builtin
         FROM exercise_library WHERE 1=1",
    );
    let mut params: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

    if let Some(ref region) = body_region {
        query.push_str(&format!(" AND body_region = ?{}", params.len() + 1));
        params.push(Box::new(region.clone()));
    }
    if let Some(ref cat) = category {
        query.push_str(&format!(" AND category = ?{}", params.len() + 1));
        params.push(Box::new(cat.clone()));
    }
    query.push_str(" ORDER BY body_region, category, name");

    let exercises = conn
        .prepare(&query)
        .map_err(|e| AppError::Database(e.to_string()))?
        .query_map(
            rusqlite::params_from_iter(params.iter().map(|p| p.as_ref())),
            |row| {
                Ok(Exercise {
                    exercise_id: row.get(0)?,
                    name: row.get(1)?,
                    body_region: row.get(2)?,
                    category: row.get(3)?,
                    description: row.get(4)?,
                    instructions: row.get(5)?,
                    equipment: row.get(6)?,
                    difficulty: row.get(7)?,
                    is_builtin: row.get::<_, i64>(8)? != 0,
                })
            },
        )
        .map_err(|e| AppError::Database(e.to_string()))?
        .filter_map(|r| r.ok())
        .collect();

    write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id: sess.user_id.clone(),
            action: "hep.list_exercises".to_string(),
            resource_type: "ExerciseLibrary".to_string(),
            resource_id: None,
            patient_id: None,
            device_id: device_id.id().to_string(),
            success: true,
            details: Some(format!(
                "region={:?},category={:?}",
                body_region, category
            )),
        },
    );

    Ok(exercises)
}

/// Search exercises by name (case-insensitive substring match).
///
/// Requires: ClinicalDocumentation + Read
#[tauri::command]
pub async fn search_exercises(
    query: String,
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
    device_id: State<'_, DeviceId>,
) -> Result<Vec<Exercise>, AppError> {
    let sess = middleware::require_authenticated(&session)?;
    middleware::require_permission(sess.role, Resource::ClinicalDocumentation, Action::Read)?;

    let conn = db.conn.lock().map_err(|e| AppError::Database(e.to_string()))?;
    seed_exercise_library(&conn)?;

    let pattern = format!("%{}%", query.to_lowercase());

    let exercises = conn
        .prepare(
            "SELECT exercise_id, name, body_region, category, description, instructions, equipment, difficulty, is_builtin
             FROM exercise_library
             WHERE lower(name) LIKE ?1 OR lower(description) LIKE ?1
             ORDER BY name",
        )
        .map_err(|e| AppError::Database(e.to_string()))?
        .query_map(rusqlite::params![pattern], |row| {
            Ok(Exercise {
                exercise_id: row.get(0)?,
                name: row.get(1)?,
                body_region: row.get(2)?,
                category: row.get(3)?,
                description: row.get(4)?,
                instructions: row.get(5)?,
                equipment: row.get(6)?,
                difficulty: row.get(7)?,
                is_builtin: row.get::<_, i64>(8)? != 0,
            })
        })
        .map_err(|e| AppError::Database(e.to_string()))?
        .filter_map(|r| r.ok())
        .collect();

    write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id: sess.user_id.clone(),
            action: "hep.search_exercises".to_string(),
            resource_type: "ExerciseLibrary".to_string(),
            resource_id: None,
            patient_id: None,
            device_id: device_id.id().to_string(),
            success: true,
            details: Some(format!("query={}", query)),
        },
    );

    Ok(exercises)
}

/// Create a new HEP program for a patient encounter.
///
/// Requires: ClinicalDocumentation + Create
#[tauri::command]
pub async fn create_hep_program(
    input: CreateHepProgramInput,
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
    device_id: State<'_, DeviceId>,
) -> Result<HepProgram, AppError> {
    let sess = middleware::require_authenticated(&session)?;
    middleware::require_permission(sess.role, Resource::ClinicalDocumentation, Action::Create)?;

    if input.exercises.is_empty() {
        return Err(AppError::Validation(
            "HEP program must contain at least one exercise".to_string(),
        ));
    }

    let program_id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();

    // Store program as a FHIR CarePlan resource
    let fhir_id = uuid::Uuid::new_v4().to_string();
    let fhir = serde_json::json!({
        "resourceType": "CarePlan",
        "id": fhir_id,
        "status": "active",
        "intent": "plan",
        "title": "Home Exercise Program",
        "subject": { "reference": format!("Patient/{}", input.patient_id) },
        "created": now,
        "category": [{
            "coding": [{
                "system": "http://medarc.local/fhir/CodeSystem/care-plan-type",
                "code": "hep",
                "display": "Home Exercise Program"
            }]
        }]
    });
    let fhir_json =
        serde_json::to_string(&fhir).map_err(|e| AppError::Serialization(e.to_string()))?;

    let exercises_json = serde_json::to_string(&input.exercises)
        .map_err(|e| AppError::Serialization(e.to_string()))?;

    let conn = db.conn.lock().map_err(|e| AppError::Database(e.to_string()))?;

    conn.execute(
        "INSERT INTO fhir_resources (id, resource_type, resource, version_id, last_updated, created_at, updated_at)
         VALUES (?1, 'HEPProgram', ?2, 1, ?3, ?3, ?3)",
        rusqlite::params![fhir_id, fhir_json, now],
    )
    .map_err(|e| AppError::Database(e.to_string()))?;

    conn.execute(
        "INSERT INTO hep_programs (program_id, resource_id, patient_id, encounter_id, created_by, exercises_json, notes, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8)",
        rusqlite::params![
            program_id,
            fhir_id,
            input.patient_id,
            input.encounter_id,
            sess.user_id,
            exercises_json,
            input.notes,
            now,
        ],
    )
    .map_err(|e| AppError::Database(e.to_string()))?;

    write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id: sess.user_id.clone(),
            action: "hep.create_program".to_string(),
            resource_type: "HEPProgram".to_string(),
            resource_id: Some(program_id.clone()),
            patient_id: Some(input.patient_id.clone()),
            device_id: device_id.id().to_string(),
            success: true,
            details: Some(format!("exercise_count={}", input.exercises.len())),
        },
    );

    Ok(HepProgram {
        program_id,
        patient_id: input.patient_id,
        encounter_id: input.encounter_id,
        created_by: sess.user_id,
        exercises: input.exercises,
        notes: input.notes,
        created_at: now.clone(),
        updated_at: now,
    })
}

/// Retrieve a single HEP program by ID.
///
/// Requires: ClinicalDocumentation + Read
#[tauri::command]
pub async fn get_hep_program(
    program_id: String,
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
    device_id: State<'_, DeviceId>,
) -> Result<HepProgram, AppError> {
    let sess = middleware::require_authenticated(&session)?;
    middleware::require_permission(sess.role, Resource::ClinicalDocumentation, Action::Read)?;

    let conn = db.conn.lock().map_err(|e| AppError::Database(e.to_string()))?;

    let (patient_id, encounter_id, created_by, exercises_json, notes, created_at, updated_at): (
        String,
        Option<String>,
        String,
        String,
        Option<String>,
        String,
        String,
    ) = conn
        .query_row(
            "SELECT patient_id, encounter_id, created_by, exercises_json, notes, created_at, updated_at
             FROM hep_programs WHERE program_id = ?1",
            rusqlite::params![program_id],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?,
                ))
            },
        )
        .map_err(|_| AppError::NotFound(format!("HEP program {} not found", program_id)))?;

    let exercises: Vec<ExercisePrescription> = serde_json::from_str(&exercises_json)
        .map_err(|e| AppError::Serialization(e.to_string()))?;

    write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id: sess.user_id.clone(),
            action: "hep.get_program".to_string(),
            resource_type: "HEPProgram".to_string(),
            resource_id: Some(program_id.clone()),
            patient_id: Some(patient_id.clone()),
            device_id: device_id.id().to_string(),
            success: true,
            details: None,
        },
    );

    Ok(HepProgram {
        program_id,
        patient_id,
        encounter_id,
        created_by,
        exercises,
        notes,
        created_at,
        updated_at,
    })
}

/// List all HEP programs for a patient.
///
/// Requires: ClinicalDocumentation + Read
#[tauri::command]
pub async fn list_hep_programs(
    patient_id: String,
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
    device_id: State<'_, DeviceId>,
) -> Result<Vec<HepProgram>, AppError> {
    let sess = middleware::require_authenticated(&session)?;
    middleware::require_permission(sess.role, Resource::ClinicalDocumentation, Action::Read)?;

    let conn = db.conn.lock().map_err(|e| AppError::Database(e.to_string()))?;

    let programs = conn
        .prepare(
            "SELECT program_id, patient_id, encounter_id, created_by, exercises_json, notes, created_at, updated_at
             FROM hep_programs WHERE patient_id = ?1 ORDER BY created_at DESC",
        )
        .map_err(|e| AppError::Database(e.to_string()))?
        .query_map(rusqlite::params![patient_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, Option<String>>(5)?,
                row.get::<_, String>(6)?,
                row.get::<_, String>(7)?,
            ))
        })
        .map_err(|e| AppError::Database(e.to_string()))?
        .filter_map(|r| r.ok())
        .filter_map(|(pid, pat_id, enc_id, created_by, ex_json, notes, created_at, updated_at)| {
            let exercises: Vec<ExercisePrescription> =
                serde_json::from_str(&ex_json).unwrap_or_default();
            Some(HepProgram {
                program_id: pid,
                patient_id: pat_id,
                encounter_id: enc_id,
                created_by,
                exercises,
                notes,
                created_at,
                updated_at,
            })
        })
        .collect();

    write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id: sess.user_id.clone(),
            action: "hep.list_programs".to_string(),
            resource_type: "HEPProgram".to_string(),
            resource_id: None,
            patient_id: Some(patient_id.clone()),
            device_id: device_id.id().to_string(),
            success: true,
            details: None,
        },
    );

    Ok(programs)
}

/// Update an existing HEP program's exercise list.
///
/// Requires: ClinicalDocumentation + Update
#[tauri::command]
pub async fn update_hep_program(
    program_id: String,
    exercises: Vec<ExercisePrescription>,
    notes: Option<String>,
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
    device_id: State<'_, DeviceId>,
) -> Result<HepProgram, AppError> {
    let sess = middleware::require_authenticated(&session)?;
    middleware::require_permission(sess.role, Resource::ClinicalDocumentation, Action::Update)?;

    if exercises.is_empty() {
        return Err(AppError::Validation(
            "HEP program must contain at least one exercise".to_string(),
        ));
    }

    let conn = db.conn.lock().map_err(|e| AppError::Database(e.to_string()))?;

    // Verify program exists
    let (patient_id, encounter_id, created_by, created_at): (String, Option<String>, String, String) = conn
        .query_row(
            "SELECT patient_id, encounter_id, created_by, created_at FROM hep_programs WHERE program_id = ?1",
            rusqlite::params![program_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .map_err(|_| AppError::NotFound(format!("HEP program {} not found", program_id)))?;

    let now = chrono::Utc::now().to_rfc3339();
    let exercises_json =
        serde_json::to_string(&exercises).map_err(|e| AppError::Serialization(e.to_string()))?;

    conn.execute(
        "UPDATE hep_programs SET exercises_json = ?1, notes = ?2, updated_at = ?3 WHERE program_id = ?4",
        rusqlite::params![exercises_json, notes, now, program_id],
    )
    .map_err(|e| AppError::Database(e.to_string()))?;

    write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id: sess.user_id.clone(),
            action: "hep.update_program".to_string(),
            resource_type: "HEPProgram".to_string(),
            resource_id: Some(program_id.clone()),
            patient_id: Some(patient_id.clone()),
            device_id: device_id.id().to_string(),
            success: true,
            details: Some(format!("exercise_count={}", exercises.len())),
        },
    );

    Ok(HepProgram {
        program_id,
        patient_id,
        encounter_id,
        created_by,
        exercises,
        notes,
        created_at,
        updated_at: now,
    })
}

/// Save a HEP program as a reusable template.
///
/// Requires: ClinicalDocumentation + Create
#[tauri::command]
pub async fn create_hep_template(
    input: CreateHepTemplateInput,
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
    device_id: State<'_, DeviceId>,
) -> Result<HepTemplate, AppError> {
    let sess = middleware::require_authenticated(&session)?;
    middleware::require_permission(sess.role, Resource::ClinicalDocumentation, Action::Create)?;

    if input.name.trim().is_empty() {
        return Err(AppError::Validation(
            "Template name cannot be empty".to_string(),
        ));
    }
    if input.exercises.is_empty() {
        return Err(AppError::Validation(
            "Template must contain at least one exercise".to_string(),
        ));
    }

    let template_id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    let exercises_json = serde_json::to_string(&input.exercises)
        .map_err(|e| AppError::Serialization(e.to_string()))?;

    let conn = db.conn.lock().map_err(|e| AppError::Database(e.to_string()))?;

    conn.execute(
        "INSERT INTO hep_templates (template_id, name, body_region, condition_name, exercises_json, created_by, is_builtin, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, 0, ?7)",
        rusqlite::params![
            template_id,
            input.name,
            input.body_region,
            input.condition_name,
            exercises_json,
            sess.user_id,
            now,
        ],
    )
    .map_err(|e| AppError::Database(e.to_string()))?;

    write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id: sess.user_id.clone(),
            action: "hep.create_template".to_string(),
            resource_type: "HEPTemplate".to_string(),
            resource_id: Some(template_id.clone()),
            patient_id: None,
            device_id: device_id.id().to_string(),
            success: true,
            details: Some(format!("name={}", input.name)),
        },
    );

    Ok(HepTemplate {
        template_id,
        name: input.name,
        body_region: input.body_region,
        condition_name: input.condition_name,
        exercises: input.exercises,
        created_by: sess.user_id,
        is_builtin: false,
        created_at: now,
    })
}

/// List all HEP templates (built-in and user-created).
///
/// Requires: ClinicalDocumentation + Read
#[tauri::command]
pub async fn list_hep_templates(
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
    device_id: State<'_, DeviceId>,
) -> Result<Vec<HepTemplate>, AppError> {
    let sess = middleware::require_authenticated(&session)?;
    middleware::require_permission(sess.role, Resource::ClinicalDocumentation, Action::Read)?;

    let conn = db.conn.lock().map_err(|e| AppError::Database(e.to_string()))?;

    // Ensure built-in templates are seeded
    seed_exercise_library(&conn)?;
    seed_builtin_templates(&conn)?;

    let templates = conn
        .prepare(
            "SELECT template_id, name, body_region, condition_name, exercises_json, created_by, is_builtin, created_at
             FROM hep_templates ORDER BY is_builtin DESC, name ASC",
        )
        .map_err(|e| AppError::Database(e.to_string()))?
        .query_map(rusqlite::params![], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, i64>(6)?,
                row.get::<_, String>(7)?,
            ))
        })
        .map_err(|e| AppError::Database(e.to_string()))?
        .filter_map(|r| r.ok())
        .filter_map(|(tid, name, region, condition, ex_json, created_by, is_builtin, created_at)| {
            let exercises: Vec<ExercisePrescription> =
                serde_json::from_str(&ex_json).unwrap_or_default();
            Some(HepTemplate {
                template_id: tid,
                name,
                body_region: region,
                condition_name: condition,
                exercises,
                created_by,
                is_builtin: is_builtin != 0,
                created_at,
            })
        })
        .collect();

    write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id: sess.user_id.clone(),
            action: "hep.list_templates".to_string(),
            resource_type: "HEPTemplate".to_string(),
            resource_id: None,
            patient_id: None,
            device_id: device_id.id().to_string(),
            success: true,
            details: None,
        },
    );

    Ok(templates)
}

/// Get a single HEP template by ID.
///
/// Requires: ClinicalDocumentation + Read
#[tauri::command]
pub async fn get_hep_template(
    template_id: String,
    db: State<'_, Database>,
    session: State<'_, SessionManager>,
    device_id: State<'_, DeviceId>,
) -> Result<HepTemplate, AppError> {
    let sess = middleware::require_authenticated(&session)?;
    middleware::require_permission(sess.role, Resource::ClinicalDocumentation, Action::Read)?;

    let conn = db.conn.lock().map_err(|e| AppError::Database(e.to_string()))?;

    let (name, body_region, condition_name, exercises_json, created_by, is_builtin, created_at): (
        String,
        Option<String>,
        Option<String>,
        String,
        String,
        i64,
        String,
    ) = conn
        .query_row(
            "SELECT name, body_region, condition_name, exercises_json, created_by, is_builtin, created_at
             FROM hep_templates WHERE template_id = ?1",
            rusqlite::params![template_id],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?,
                ))
            },
        )
        .map_err(|_| AppError::NotFound(format!("HEP template {} not found", template_id)))?;

    let exercises: Vec<ExercisePrescription> = serde_json::from_str(&exercises_json)
        .map_err(|e| AppError::Serialization(e.to_string()))?;

    write_audit_entry(
        &conn,
        AuditEntryInput {
            user_id: sess.user_id.clone(),
            action: "hep.get_template".to_string(),
            resource_type: "HEPTemplate".to_string(),
            resource_id: Some(template_id.clone()),
            patient_id: None,
            device_id: device_id.id().to_string(),
            success: true,
            details: None,
        },
    );

    Ok(HepTemplate {
        template_id,
        name,
        body_region,
        condition_name,
        exercises,
        created_by,
        is_builtin: is_builtin != 0,
        created_at,
    })
}

// ─────────────────────────────────────────────────────────────────────────────
// Unit tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// Verify the seed library contains at least 50 exercises.
    #[test]
    fn exercise_seed_count_at_least_50() {
        assert!(
            EXERCISE_SEEDS.len() >= 50,
            "Expected ≥50 exercises in EXERCISE_SEEDS, got {}",
            EXERCISE_SEEDS.len()
        );
    }

    /// Verify all expected body regions are represented in the seed data.
    #[test]
    fn exercise_seed_covers_all_body_regions() {
        let regions: std::collections::HashSet<&str> =
            EXERCISE_SEEDS.iter().map(|e| e.body_region).collect();

        for expected in &[
            "cervical",
            "thoracic",
            "lumbar",
            "shoulder",
            "elbow",
            "wrist",
            "hip",
            "knee",
            "ankle",
            "general",
        ] {
            assert!(
                regions.contains(expected),
                "Body region '{}' missing from seed data",
                expected
            );
        }
    }

    /// Verify each seed exercise has a unique ID.
    #[test]
    fn exercise_seed_ids_are_unique() {
        let mut ids = std::collections::HashSet::new();
        for ex in EXERCISE_SEEDS {
            assert!(
                ids.insert(ex.id),
                "Duplicate exercise ID found: {}",
                ex.id
            );
        }
    }

    /// Verify ExercisePrescription serializes to camelCase JSON correctly.
    #[test]
    fn exercise_prescription_serializes_to_camel_case() {
        let p = ExercisePrescription {
            exercise_id: "ex-knee-01".to_string(),
            sets: Some(3),
            reps: Some(10),
            duration_seconds: None,
            hold_seconds: Some(5),
            times_per_day: Some(2),
            days_per_week: Some(7),
            resistance: Some("yellow band".to_string()),
            pain_limit: Some(4),
            notes: Some("Go slow".to_string()),
        };

        let json_str = serde_json::to_string(&p).unwrap();
        let val: serde_json::Value = serde_json::from_str(&json_str).unwrap();
        let obj = val.as_object().unwrap();

        assert!(obj.contains_key("exerciseId"), "missing exerciseId");
        assert!(obj.contains_key("sets"), "missing sets");
        assert!(obj.contains_key("reps"), "missing reps");
        assert!(obj.contains_key("holdSeconds"), "missing holdSeconds");
        assert!(obj.contains_key("timesPerDay"), "missing timesPerDay");
        assert!(obj.contains_key("daysPerWeek"), "missing daysPerWeek");
        assert!(obj.contains_key("painLimit"), "missing painLimit");
        assert_eq!(obj["exerciseId"].as_str().unwrap(), "ex-knee-01");
        assert_eq!(obj["sets"].as_u64().unwrap(), 3);
    }

    /// Verify HepTemplate round-trips through JSON correctly.
    #[test]
    fn hep_template_round_trips_json() {
        let exercises = vec![
            ExercisePrescription {
                exercise_id: "ex-lumb-01".to_string(),
                sets: Some(3),
                reps: Some(10),
                duration_seconds: None,
                hold_seconds: None,
                times_per_day: Some(1),
                days_per_week: Some(7),
                resistance: None,
                pain_limit: Some(4),
                notes: None,
            },
            ExercisePrescription {
                exercise_id: "ex-lumb-04".to_string(),
                sets: Some(3),
                reps: Some(10),
                duration_seconds: None,
                hold_seconds: None,
                times_per_day: Some(1),
                days_per_week: Some(7),
                resistance: None,
                pain_limit: Some(4),
                notes: None,
            },
        ];

        let template = HepTemplate {
            template_id: "tmpl-test-001".to_string(),
            name: "Test Template".to_string(),
            body_region: Some("lumbar".to_string()),
            condition_name: Some("Low back pain".to_string()),
            exercises: exercises.clone(),
            created_by: "user-001".to_string(),
            is_builtin: false,
            created_at: "2026-01-01T00:00:00Z".to_string(),
        };

        let json_str = serde_json::to_string(&template).unwrap();
        let restored: HepTemplate = serde_json::from_str(&json_str).unwrap();

        assert_eq!(restored.template_id, template.template_id);
        assert_eq!(restored.name, template.name);
        assert_eq!(restored.exercises.len(), 2);
        assert_eq!(
            restored.exercises[0].exercise_id,
            "ex-lumb-01"
        );
        assert_eq!(
            restored.exercises[1].exercise_id,
            "ex-lumb-04"
        );
        assert!(!restored.is_builtin);
    }

    /// Verify that MIGRATIONS validates correctly (regression guard for migration 23).
    #[test]
    fn migrations_validate_with_hep_tables() {
        use crate::db::migrations::MIGRATIONS;
        assert!(
            MIGRATIONS.validate().is_ok(),
            "MIGRATIONS.validate() failed — check Migration 23 SQL syntax"
        );
    }

    /// Verify that seed data has valid body region values matching the DB CHECK constraint.
    #[test]
    fn exercise_seeds_have_valid_body_regions() {
        let valid_regions = [
            "cervical",
            "thoracic",
            "lumbar",
            "shoulder",
            "elbow",
            "wrist",
            "hip",
            "knee",
            "ankle",
            "general",
        ];
        for ex in EXERCISE_SEEDS {
            assert!(
                valid_regions.contains(&ex.body_region),
                "Exercise '{}' has invalid body_region '{}'",
                ex.name,
                ex.body_region
            );
        }
    }

    /// Verify that seed data has valid category values matching the DB CHECK constraint.
    #[test]
    fn exercise_seeds_have_valid_categories() {
        let valid_categories = [
            "rom",
            "strengthening",
            "stretching",
            "balance",
            "functional",
            "cardio",
        ];
        for ex in EXERCISE_SEEDS {
            assert!(
                valid_categories.contains(&ex.category),
                "Exercise '{}' has invalid category '{}'",
                ex.name,
                ex.category
            );
        }
    }
}
