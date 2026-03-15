/**
 * SurveyKioskPage.tsx — Patient-facing survey kiosk view.
 *
 * Designed for tablet/kiosk use: LARGE elements, high contrast, no sidebar,
 * no navigation chrome. Shows one field per section with a progress bar at top.
 *
 * Field types rendered:
 *   - Text      -> large textarea
 *   - Number    -> large number input with +/- buttons
 *   - YesNo     -> two large toggle buttons (Yes / No)
 *   - PainScale -> 0-10 horizontal scale with color gradient (green->yellow->red)
 *   - Date      -> date picker
 *
 * On submit: saves via submitSurveyResponse, shows a thank-you screen.
 */

import { useState, useEffect, useCallback } from "react";
import {
  getSurveyTemplate,
  submitSurveyResponse,
} from "../lib/surveyStore";
import type {
  SurveyTemplate,
  SurveyField,
  SurveyFieldResponse,
} from "../types/survey";

// ─── Props ──────────────────────────────────────────────────────────────────

interface SurveyKioskPageProps {
  patientId: string;
  templateId: string;
}

// ─── Pain scale color helpers ───────────────────────────────────────────────

/**
 * Returns a background color class for a pain scale value 0-10.
 * Gradient: green (0) -> yellow (5) -> red (10).
 */
function painScaleBg(value: number, selected: boolean): string {
  if (!selected) return "bg-gray-100 text-gray-500 hover:bg-gray-200";

  if (value <= 1) return "bg-green-500 text-white";
  if (value <= 2) return "bg-green-400 text-white";
  if (value <= 3) return "bg-lime-400 text-white";
  if (value <= 4) return "bg-yellow-300 text-gray-900";
  if (value <= 5) return "bg-yellow-400 text-gray-900";
  if (value <= 6) return "bg-amber-400 text-white";
  if (value <= 7) return "bg-orange-400 text-white";
  if (value <= 8) return "bg-orange-500 text-white";
  if (value <= 9) return "bg-red-500 text-white";
  return "bg-red-600 text-white";
}

// ─── Field renderers ────────────────────────────────────────────────────────

interface FieldInputProps {
  field: SurveyField;
  value: string;
  onChange: (value: string) => void;
}

function TextFieldInput({ field, value, onChange }: FieldInputProps) {
  const inputId = `survey-field-${field.id}`;
  return (
    <div>
      <label htmlFor={inputId} className="mb-3 block text-xl font-semibold text-gray-800">
        {field.label}
        {field.required && <span className="ml-1 text-red-500">*</span>}
      </label>
      <textarea
        id={inputId}
        value={value}
        onChange={(e) => onChange(e.target.value)}
        placeholder="Type your answer here..."
        rows={4}
        className="w-full rounded-xl border-2 border-gray-300 px-5 py-4 text-lg shadow-sm focus:border-blue-500 focus:outline-none focus:ring-2 focus:ring-blue-500"
      />
    </div>
  );
}

function NumberFieldInput({ field, value, onChange }: FieldInputProps) {
  const numValue = value === "" ? 0 : parseInt(value, 10);
  const displayValue = value === "" ? "" : String(numValue);
  const inputId = `survey-field-${field.id}`;

  const handleDecrement = () => {
    onChange(String(Math.max(0, numValue - 1)));
  };

  const handleIncrement = () => {
    onChange(String(numValue + 1));
  };

  return (
    <div>
      <label htmlFor={inputId} className="mb-3 block text-xl font-semibold text-gray-800">
        {field.label}
        {field.required && <span className="ml-1 text-red-500">*</span>}
      </label>
      <div className="flex items-center gap-4">
        <button
          type="button"
          onClick={handleDecrement}
          aria-label={`Decrease ${field.label}`}
          className="flex h-16 w-16 items-center justify-center rounded-xl border-2 border-gray-300 bg-gray-50 text-3xl font-bold text-gray-700 shadow-sm hover:bg-gray-100 active:bg-gray-200"
        >
          -
        </button>
        <input
          id={inputId}
          type="number"
          value={displayValue}
          onChange={(e) => onChange(e.target.value)}
          min={0}
          className="h-16 w-32 rounded-xl border-2 border-gray-300 text-center text-2xl font-semibold shadow-sm focus:border-blue-500 focus:outline-none focus:ring-2 focus:ring-blue-500"
        />
        <button
          type="button"
          onClick={handleIncrement}
          aria-label={`Increase ${field.label}`}
          className="flex h-16 w-16 items-center justify-center rounded-xl border-2 border-gray-300 bg-gray-50 text-3xl font-bold text-gray-700 shadow-sm hover:bg-gray-100 active:bg-gray-200"
        >
          +
        </button>
      </div>
    </div>
  );
}

function YesNoFieldInput({ field, value, onChange }: FieldInputProps) {
  return (
    <div>
      <p className="mb-4 block text-xl font-semibold text-gray-800" id={`survey-field-label-${field.id}`}>
        {field.label}
        {field.required && <span className="ml-1 text-red-500">*</span>}
      </p>
      <div className="flex gap-4" role="group" aria-labelledby={`survey-field-label-${field.id}`}>
        <button
          type="button"
          onClick={() => onChange("Yes")}
          aria-pressed={value === "Yes"}
          className={[
            "flex-1 rounded-xl border-2 py-5 text-xl font-bold shadow-sm transition-colors",
            value === "Yes"
              ? "border-green-500 bg-green-500 text-white"
              : "border-gray-300 bg-white text-gray-700 hover:bg-green-50 hover:border-green-400",
          ].join(" ")}
        >
          Yes
        </button>
        <button
          type="button"
          onClick={() => onChange("No")}
          aria-pressed={value === "No"}
          className={[
            "flex-1 rounded-xl border-2 py-5 text-xl font-bold shadow-sm transition-colors",
            value === "No"
              ? "border-red-500 bg-red-500 text-white"
              : "border-gray-300 bg-white text-gray-700 hover:bg-red-50 hover:border-red-400",
          ].join(" ")}
        >
          No
        </button>
      </div>
    </div>
  );
}

function PainScaleFieldInput({ field, value, onChange }: FieldInputProps) {
  const selected = value === "" ? -1 : parseInt(value, 10);

  return (
    <div>
      <p className="mb-4 block text-xl font-semibold text-gray-800" id={`survey-field-label-${field.id}`}>
        {field.label}
        {field.required && <span className="ml-1 text-red-500">*</span>}
      </p>
      <div className="flex flex-wrap gap-2" role="group" aria-labelledby={`survey-field-label-${field.id}`}>
        {Array.from({ length: 11 }, (_, i) => (
          <button
            key={i}
            type="button"
            onClick={() => onChange(String(i))}
            className={[
              "flex h-16 w-16 items-center justify-center rounded-xl border-2 text-xl font-bold shadow-sm transition-all",
              painScaleBg(i, selected === i),
              selected === i ? "border-gray-800 scale-110" : "border-transparent",
            ].join(" ")}
            aria-label={`Pain level ${i}`}
          >
            {i}
          </button>
        ))}
      </div>
      <div className="mt-2 flex justify-between text-sm text-gray-500">
        <span>No pain</span>
        <span>Worst pain</span>
      </div>
    </div>
  );
}

function DateFieldInput({ field, value, onChange }: FieldInputProps) {
  const inputId = `survey-field-${field.id}`;
  return (
    <div>
      <label htmlFor={inputId} className="mb-3 block text-xl font-semibold text-gray-800">
        {field.label}
        {field.required && <span className="ml-1 text-red-500">*</span>}
      </label>
      <input
        id={inputId}
        type="date"
        value={value}
        onChange={(e) => onChange(e.target.value)}
        className="h-16 w-full max-w-sm rounded-xl border-2 border-gray-300 px-5 text-lg shadow-sm focus:border-blue-500 focus:outline-none focus:ring-2 focus:ring-blue-500"
      />
    </div>
  );
}

/** Dispatch to the correct field renderer based on field type. */
function SurveyFieldInput({ field, value, onChange }: FieldInputProps) {
  switch (field.fieldType) {
    case "text":
      return <TextFieldInput field={field} value={value} onChange={onChange} />;
    case "number":
      return <NumberFieldInput field={field} value={value} onChange={onChange} />;
    case "yes_no":
      return <YesNoFieldInput field={field} value={value} onChange={onChange} />;
    case "pain_scale":
      return <PainScaleFieldInput field={field} value={value} onChange={onChange} />;
    case "date":
      return <DateFieldInput field={field} value={value} onChange={onChange} />;
    default:
      return <TextFieldInput field={field} value={value} onChange={onChange} />;
  }
}

// ─── Main component ─────────────────────────────────────────────────────────

export function SurveyKioskPage({ patientId, templateId }: SurveyKioskPageProps) {
  // ── State ─────────────────────────────────────────────────────────────────
  const [template, setTemplate] = useState<SurveyTemplate | null>(null);
  const [loading, setLoading] = useState(true);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [answers, setAnswers] = useState<Record<string, string>>({});
  const [submitting, setSubmitting] = useState(false);
  const [submitError, setSubmitError] = useState<string | null>(null);
  const [submitted, setSubmitted] = useState(false);

  // ── Load template ─────────────────────────────────────────────────────────
  useEffect(() => {
    let mounted = true;
    setLoading(true);
    setLoadError(null);

    getSurveyTemplate(templateId)
      .then((result) => {
        if (!mounted) return;
        if (result === null) {
          setLoadError("Survey template not found.");
        } else {
          setTemplate(result);
          // Initialize answers map
          const initial: Record<string, string> = {};
          for (const field of result.fields) {
            initial[field.id] = "";
          }
          setAnswers(initial);
        }
      })
      .catch((e) => {
        if (!mounted) return;
        setLoadError(e instanceof Error ? e.message : String(e));
      })
      .finally(() => {
        if (mounted) setLoading(false);
      });

    return () => {
      mounted = false;
    };
  }, [templateId]);

  // ── Answer change handler ─────────────────────────────────────────────────
  const handleAnswerChange = useCallback((fieldId: string, value: string) => {
    setAnswers((prev) => ({ ...prev, [fieldId]: value }));
  }, []);

  // ── Compute progress ──────────────────────────────────────────────────────
  const sortedFields = template
    ? [...template.fields].sort((a, b) => a.order - b.order)
    : [];

  const totalFields = sortedFields.length;
  const answeredFields = sortedFields.filter(
    (f) => answers[f.id] !== undefined && answers[f.id] !== "",
  ).length;
  const progressPercent = totalFields > 0 ? Math.round((answeredFields / totalFields) * 100) : 0;

  // ── Validation ────────────────────────────────────────────────────────────
  const validate = useCallback((): string | null => {
    if (!template) return "No template loaded.";
    for (const field of sortedFields) {
      if (field.required && (!answers[field.id] || answers[field.id].trim() === "")) {
        return `"${field.label}" is required.`;
      }
    }
    return null;
  }, [template, sortedFields, answers]);

  // ── Submit handler ────────────────────────────────────────────────────────
  const handleSubmit = useCallback(async () => {
    const error = validate();
    if (error) {
      setSubmitError(error);
      return;
    }

    setSubmitting(true);
    setSubmitError(null);

    try {
      const responses: SurveyFieldResponse[] = sortedFields.map((field) => ({
        fieldId: field.id,
        value: answers[field.id] ?? "",
      }));

      await submitSurveyResponse({
        templateId,
        patientId,
        responses,
      });

      setSubmitted(true);
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      setSubmitError(msg);
    } finally {
      setSubmitting(false);
    }
  }, [validate, sortedFields, answers, templateId, patientId]);

  // ── Loading state ─────────────────────────────────────────────────────────
  if (loading) {
    return (
      <div className="flex min-h-screen items-center justify-center bg-gray-50">
        <div className="animate-pulse text-center">
          <div className="mx-auto h-12 w-48 rounded bg-gray-200" />
          <div className="mt-4 mx-auto h-6 w-64 rounded bg-gray-200" />
        </div>
      </div>
    );
  }

  // ── Error state ───────────────────────────────────────────────────────────
  if (loadError || !template) {
    return (
      <div className="flex min-h-screen items-center justify-center bg-gray-50 p-8">
        <div className="max-w-md rounded-2xl border border-red-200 bg-white p-8 text-center shadow-lg">
          <h1 className="mb-4 text-2xl font-bold text-red-600">
            Unable to Load Survey
          </h1>
          <p className="text-lg text-gray-600">
            {loadError ?? "Survey template not found."}
          </p>
          <p className="mt-4 text-sm text-gray-400">
            Please ask the front desk for assistance.
          </p>
        </div>
      </div>
    );
  }

  // ── Success / thank-you state ─────────────────────────────────────────────
  if (submitted) {
    return (
      <div className="flex min-h-screen items-center justify-center bg-green-50 p-8">
        <div className="max-w-lg rounded-2xl border border-green-200 bg-white p-12 text-center shadow-lg">
          <div className="mx-auto mb-6 flex h-20 w-20 items-center justify-center rounded-full bg-green-100">
            <svg
              className="h-10 w-10 text-green-600"
              fill="none"
              viewBox="0 0 24 24"
              strokeWidth="2.5"
              stroke="currentColor"
            >
              <path
                strokeLinecap="round"
                strokeLinejoin="round"
                d="M4.5 12.75l6 6 9-13.5"
              />
            </svg>
          </div>
          <h1 className="mb-4 text-3xl font-bold text-gray-900">
            Thank You!
          </h1>
          <p className="text-xl text-gray-600">
            Your responses have been submitted successfully.
          </p>
          <p className="mt-6 text-lg font-medium text-gray-500">
            Please return this device to the front desk.
          </p>
        </div>
      </div>
    );
  }

  // ── Survey form ───────────────────────────────────────────────────────────
  return (
    <div className="min-h-screen bg-gray-50">
      {/* Progress bar */}
      <div className="sticky top-0 z-10 bg-white shadow-sm">
        <div
          className="h-2 w-full bg-gray-200"
          role="progressbar"
          aria-valuenow={progressPercent}
          aria-valuemin={0}
          aria-valuemax={100}
          aria-label={`Survey progress: ${progressPercent}%`}
        >
          <div
            className="h-full bg-blue-500 transition-all duration-300"
            style={{ width: `${progressPercent}%` }}
          />
        </div>
        <div className="flex items-center justify-between px-6 py-4">
          <h1 className="text-2xl font-bold text-gray-900">{template.name}</h1>
          <span className="text-sm font-medium text-gray-500">
            {answeredFields} of {totalFields} completed
          </span>
        </div>
      </div>

      {/* Fields */}
      <div className="mx-auto max-w-2xl space-y-8 px-6 py-8">
        {sortedFields.map((field) => (
          <div
            key={field.id}
            className="rounded-2xl border border-gray-200 bg-white p-6 shadow-sm"
          >
            <SurveyFieldInput
              field={field}
              value={answers[field.id] ?? ""}
              onChange={(value) => handleAnswerChange(field.id, value)}
            />
          </div>
        ))}

        {/* Submit error */}
        {submitError && (
          <div className="rounded-xl border-2 border-red-200 bg-red-50 px-6 py-4 text-center text-lg text-red-700">
            {submitError}
          </div>
        )}

        {/* Submit button */}
        <div className="pb-12 pt-4">
          <button
            type="button"
            onClick={handleSubmit}
            disabled={submitting}
            aria-label={submitting ? "Submitting survey, please wait" : "Submit survey"}
            aria-busy={submitting}
            aria-disabled={submitting}
            className="w-full rounded-2xl bg-blue-600 py-5 text-xl font-bold text-white shadow-lg transition-colors hover:bg-blue-700 active:bg-blue-800 disabled:opacity-60"
          >
            {submitting ? "Submitting..." : "Submit Survey"}
          </button>
        </div>
      </div>
    </div>
  );
}
