/**
 * PatientAutocomplete.tsx — Reusable patient typeahead/autocomplete component.
 *
 * Usage:
 *   <PatientAutocomplete value={patientId} onChange={setPatientId} />
 *
 * Behaviour:
 *  - As the user types (≥ 2 characters), fires commands.searchPatients after
 *    a 300 ms debounce and shows a dropdown of up to 10 matches.
 *  - Each row displays: full name, DOB, MRN.
 *  - Selecting a row stores the patient ID as the value and shows the patient
 *    name in the input field.
 *  - A clear button (✕) appears when a patient is selected.
 *  - Keyboard navigation: ArrowDown/ArrowUp move the highlight, Enter selects,
 *    Escape closes the dropdown.
 *  - Loading and empty-results states are handled explicitly.
 *
 * Styling uses the same Tailwind classes as the rest of the scheduling forms.
 */
import { useState, useEffect, useRef, useCallback } from "react";
import { commands } from "../../lib/tauri";
import type { PatientSummary } from "../../types/patient";

// ─── Shared style constants (mirrors AppointmentFormModal) ────────────────────

const INPUT_CLS =
  "w-full rounded-md border border-gray-300 px-3 py-2 text-sm shadow-sm focus:border-blue-500 focus:outline-none focus:ring-1 focus:ring-blue-500";

// ─── Helpers ──────────────────────────────────────────────────────────────────

/** Format an ISO date "YYYY-MM-DD" → "MM/DD/YYYY", or "—" if null. */
function formatDate(iso: string | null): string {
  if (!iso) return "—";
  const parts = iso.split("-");
  if (parts.length !== 3) return iso;
  return `${parts[1]}/${parts[2]}/${parts[0]}`;
}

/** Return the full display name from a PatientSummary. */
function displayName(p: PatientSummary): string {
  return `${p.givenNames.join(" ")} ${p.familyName}`.trim();
}

// ─── Props ────────────────────────────────────────────────────────────────────

export interface PatientAutocompleteProps {
  /** Currently selected patient ID, or null when nothing is selected. */
  value: string | null;
  /** Called with the selected patient ID, or null when the selection is cleared. */
  onChange: (patientId: string | null) => void;
  placeholder?: string;
  disabled?: boolean;
  /** Called when the user clicks "Create new patient" from the empty-results state. */
  onCreatePatient?: (name: string) => void;
  /** Auto-focus the input field when the component mounts. */
  autoFocus?: boolean;
}

// ─── Component ────────────────────────────────────────────────────────────────

export function PatientAutocomplete({
  value,
  onChange,
  placeholder = "Search by name…",
  disabled = false,
  onCreatePatient,
  autoFocus = false,
}: PatientAutocompleteProps) {
  // Text shown in the input field.
  const [inputText, setInputText] = useState("");
  // Dropdown search results.
  const [results, setResults] = useState<PatientSummary[]>([]);
  // True while the backend call is in flight.
  const [loading, setLoading] = useState(false);
  // True when the dropdown should be visible.
  const [open, setOpen] = useState(false);
  // Index of the keyboard-highlighted row (-1 = none).
  const [highlightIdx, setHighlightIdx] = useState(-1);
  // Cache of id → display name so we can restore the label when value changes.
  const [selectedName, setSelectedName] = useState<string | null>(null);

  const containerRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLInputElement>(null);

  // ── Reliable auto-focus (handles modal rendering timing) ─────────────────
  useEffect(() => {
    if (autoFocus && inputRef.current) {
      // Small delay to ensure modal/DOM is fully mounted
      const timer = setTimeout(() => inputRef.current?.focus(), 50);
      return () => clearTimeout(timer);
    }
  }, [autoFocus]);

  // ── Sync display text when value is controlled externally ────────────────
  useEffect(() => {
    if (value === null) {
      if (!open) {
        setInputText("");
      }
      setSelectedName(null);
    } else if (selectedName && !open) {
      setInputText(selectedName);
    }
  }, [value, selectedName, open]);

  // ── Debounced search ─────────────────────────────────────────────────────
  useEffect(() => {
    // Only search when there is no active selection AND input is long enough.
    if (value !== null) return;
    if (inputText.trim().length < 2) {
      setResults([]);
      setOpen(false);
      return;
    }

    const timer = setTimeout(async () => {
      setLoading(true);
      try {
        const hits = await commands.searchPatients({
          name: inputText.trim(),
          mrn: null,
          birthDate: null,
          limit: 10,
        });
        setResults(hits);
        setOpen(true);
        setHighlightIdx(hits.length > 0 ? 0 : -1);
      } catch {
        setResults([]);
        setOpen(false);
      } finally {
        setLoading(false);
      }
    }, 300);

    return () => clearTimeout(timer);
  }, [inputText, value]);

  // ── Close dropdown when clicking outside ────────────────────────────────
  useEffect(() => {
    function handleOutsideClick(e: MouseEvent) {
      if (
        containerRef.current &&
        !containerRef.current.contains(e.target as Node)
      ) {
        setOpen(false);
      }
    }
    document.addEventListener("mousedown", handleOutsideClick);
    return () => document.removeEventListener("mousedown", handleOutsideClick);
  }, []);

  // ── Selection ────────────────────────────────────────────────────────────
  const selectPatient = useCallback(
    (patient: PatientSummary) => {
      const name = displayName(patient);
      setInputText(name);
      setSelectedName(name);
      setOpen(false);
      setResults([]);
      onChange(patient.id);
    },
    [onChange],
  );

  // ── Clear ────────────────────────────────────────────────────────────────
  const clearSelection = useCallback(() => {
    setInputText("");
    setSelectedName(null);
    setResults([]);
    setOpen(false);
    onChange(null);
    // Return focus to the input so the user can immediately type again.
    inputRef.current?.focus();
  }, [onChange]);

  // ── Keyboard navigation ──────────────────────────────────────────────────
  function handleKeyDown(e: React.KeyboardEvent<HTMLInputElement>) {
    if (!open || results.length === 0) {
      if (e.key === "Escape") setOpen(false);
      return;
    }

    switch (e.key) {
      case "ArrowDown":
        e.preventDefault();
        setHighlightIdx((i) => Math.min(i + 1, results.length - 1));
        break;
      case "ArrowUp":
        e.preventDefault();
        setHighlightIdx((i) => Math.max(i - 1, 0));
        break;
      case "Enter":
        e.preventDefault();
        if (highlightIdx >= 0 && highlightIdx < results.length) {
          selectPatient(results[highlightIdx]);
        }
        break;
    }
  }

  // ── Input change ─────────────────────────────────────────────────────────
  function handleInputChange(e: React.ChangeEvent<HTMLInputElement>) {
    const text = e.target.value;
    setInputText(text);

    // If the user edits the text after a selection, clear the selection so
    // the parent knows no patient is chosen yet.
    if (value !== null) {
      onChange(null);
      setSelectedName(null);
    }
  }

  // ── Render ───────────────────────────────────────────────────────────────

  return (
    <div ref={containerRef} className="relative w-full">
      {/* Input row */}
      <div className="relative">
        <input
          ref={inputRef}
          type="text"
          value={inputText}
          onChange={handleInputChange}
          onKeyDown={handleKeyDown}
          onFocus={() => {
            // Re-open dropdown if there are cached results and no selection.
            if (value === null && results.length > 0) setOpen(true);
          }}
          placeholder={placeholder}
          disabled={disabled}
          autoFocus={autoFocus}
          autoComplete="off"
          aria-autocomplete="list"
          aria-expanded={open}
          aria-haspopup="listbox"
          className={`${INPUT_CLS} ${value ? "pr-8" : ""} disabled:cursor-not-allowed disabled:opacity-50`}
        />

        {/* Clear button — only shown when a patient is selected */}
        {value && !disabled && (
          <button
            type="button"
            onClick={clearSelection}
            aria-label="Clear patient selection"
            className="absolute inset-y-0 right-2 flex items-center text-gray-400 hover:text-gray-600 focus:outline-none"
          >
            ✕
          </button>
        )}

        {/* Spinner — shown while loading */}
        {loading && (
          <span className="absolute inset-y-0 right-2 flex items-center pointer-events-none">
            <svg
              className="h-4 w-4 animate-spin text-blue-500"
              xmlns="http://www.w3.org/2000/svg"
              fill="none"
              viewBox="0 0 24 24"
              aria-hidden="true"
            >
              <circle
                className="opacity-25"
                cx="12"
                cy="12"
                r="10"
                stroke="currentColor"
                strokeWidth="4"
              />
              <path
                className="opacity-75"
                fill="currentColor"
                d="M4 12a8 8 0 018-8v8H4z"
              />
            </svg>
          </span>
        )}
      </div>

      {/* Dropdown */}
      {open && !loading && (
        <ul
          role="listbox"
          className="absolute z-50 mt-1 w-full rounded-md border border-gray-200 bg-white shadow-lg max-h-60 overflow-y-auto"
        >
          {results.length === 0 ? (
            <li className="px-4 py-3 text-sm select-none">
              <span className="text-gray-500">No patients found.</span>
              {onCreatePatient && (
                <button
                  type="button"
                  onMouseDown={(e) => {
                    e.preventDefault();
                    onCreatePatient(inputText.trim());
                    setOpen(false);
                  }}
                  className="ml-2 text-blue-600 hover:text-blue-800 font-medium"
                >
                  Create &ldquo;{inputText.trim()}&rdquo; as new patient
                </button>
              )}
            </li>
          ) : (
            results.map((patient, idx) => (
              <li
                key={patient.id}
                role="option"
                aria-selected={idx === highlightIdx}
                onMouseDown={(e) => {
                  // Prevent the input blur from firing before the click.
                  e.preventDefault();
                  selectPatient(patient);
                }}
                onMouseEnter={() => setHighlightIdx(idx)}
                className={`cursor-pointer px-4 py-2.5 text-sm ${
                  idx === highlightIdx
                    ? "bg-blue-100 text-blue-900 font-semibold"
                    : "text-gray-900 hover:bg-gray-50"
                }`}
              >
                <span className="font-medium">{displayName(patient)}</span>
                <span className="ml-2 text-gray-500">
                  DOB: {formatDate(patient.birthDate)} · MRN: {patient.mrn}
                </span>
              </li>
            ))
          )}
        </ul>
      )}
    </div>
  );
}
