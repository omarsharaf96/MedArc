/**
 * SurveyBuilderPage.tsx — Intake survey template builder.
 *
 * Two views:
 *   1. Template List (default) — browse all templates, create new ones.
 *   2. Builder View — add/reorder/remove fields, configure types, save.
 *
 * Uses localStorage-backed surveyStore (will swap to Tauri invoke() when
 * Rust commands land).
 */

import { useState, useEffect, useCallback, useRef } from "react";
import { useNav } from "../contexts/RouterContext";
import {
  listSurveyTemplates,
  createSurveyTemplate,
} from "../lib/surveyStore";
import type {
  SurveyTemplate,
  SurveyField,
  SurveyFieldType,
} from "../types/survey";

// ─── Constants ──────────────────────────────────────────────────────────────

const FIELD_TYPES: { value: SurveyFieldType; label: string }[] = [
  { value: "text", label: "Text" },
  { value: "number", label: "Number" },
  { value: "yes_no", label: "Yes / No" },
  { value: "pain_scale", label: "Pain Scale (0-10)" },
  { value: "date", label: "Date" },
];

// ─── Props ──────────────────────────────────────────────────────────────────

interface SurveyBuilderPageProps {
  role: string;
  userId: string;
}

// ─── ID helper ──────────────────────────────────────────────────────────────

function generateFieldId(): string {
  return crypto.randomUUID();
}

// ─── Sub-components ─────────────────────────────────────────────────────────

/** A card-styled section wrapper (matches PatientDetailPage pattern). */
function SectionCard({
  title,
  children,
}: {
  title: string;
  children: React.ReactNode;
}) {
  return (
    <section className="rounded-lg border border-gray-200 bg-white p-5 shadow-sm">
      <h2 className="mb-4 text-base font-semibold text-gray-800">{title}</h2>
      {children}
    </section>
  );
}

// ─── Field Row Component ────────────────────────────────────────────────────

interface FieldRowProps {
  field: SurveyField;
  index: number;
  total: number;
  onChange: (id: string, updates: Partial<SurveyField>) => void;
  onMoveUp: (id: string) => void;
  onMoveDown: (id: string) => void;
  onDelete: (id: string) => void;
}

function FieldRow({
  field,
  index,
  total,
  onChange,
  onMoveUp,
  onMoveDown,
  onDelete,
}: FieldRowProps) {
  return (
    <div className="flex flex-wrap items-start gap-3 rounded-md border border-gray-200 bg-gray-50 p-4">
      {/* Order controls */}
      <div className="flex flex-col gap-1">
        <button
          type="button"
          onClick={() => onMoveUp(field.id)}
          disabled={index === 0}
          className="rounded border border-gray-300 bg-white px-2 py-1 text-xs font-medium text-gray-600 hover:bg-gray-100 disabled:cursor-not-allowed disabled:opacity-30"
          aria-label="Move field up"
        >
          ↑
        </button>
        <button
          type="button"
          onClick={() => onMoveDown(field.id)}
          disabled={index === total - 1}
          className="rounded border border-gray-300 bg-white px-2 py-1 text-xs font-medium text-gray-600 hover:bg-gray-100 disabled:cursor-not-allowed disabled:opacity-30"
          aria-label="Move field down"
        >
          ↓
        </button>
      </div>

      {/* Field number */}
      <span className="mt-2 w-6 text-center text-sm font-medium text-gray-400">
        {index + 1}
      </span>

      {/* Label input */}
      <div className="flex-1">
        <label className="mb-1 block text-xs font-medium text-gray-500">
          Label
        </label>
        <input
          type="text"
          value={field.label}
          onChange={(e) => onChange(field.id, { label: e.target.value })}
          placeholder="Field label..."
          className="w-full rounded-md border border-gray-300 px-3 py-2 text-sm shadow-sm focus:border-blue-500 focus:outline-none focus:ring-1 focus:ring-blue-500"
        />
      </div>

      {/* Type dropdown */}
      <div className="w-44">
        <label className="mb-1 block text-xs font-medium text-gray-500">
          Type
        </label>
        <select
          value={field.fieldType}
          onChange={(e) =>
            onChange(field.id, { fieldType: e.target.value as SurveyFieldType })
          }
          className="w-full rounded-md border border-gray-300 px-3 py-2 text-sm shadow-sm focus:border-blue-500 focus:outline-none focus:ring-1 focus:ring-blue-500"
        >
          {FIELD_TYPES.map((ft) => (
            <option key={ft.value} value={ft.value}>
              {ft.label}
            </option>
          ))}
        </select>
      </div>

      {/* Required checkbox */}
      <div className="flex flex-col items-center pt-5">
        <label className="flex items-center gap-1.5 text-xs text-gray-600">
          <input
            type="checkbox"
            checked={field.required}
            onChange={(e) => onChange(field.id, { required: e.target.checked })}
            className="h-4 w-4 rounded border-gray-300 text-blue-600 focus:ring-blue-500"
          />
          Required
        </label>
      </div>

      {/* Delete button */}
      <div className="pt-5">
        <button
          type="button"
          onClick={() => onDelete(field.id)}
          className="rounded-md border border-red-200 bg-red-50 px-3 py-1.5 text-xs font-medium text-red-700 hover:bg-red-100"
        >
          Delete
        </button>
      </div>
    </div>
  );
}

// ─── Main component ─────────────────────────────────────────────────────────

export function SurveyBuilderPage(_props: SurveyBuilderPageProps) {
  const { goBack } = useNav();

  // ── View state ──────────────────────────────────────────────────────────
  const [view, setView] = useState<"list" | "builder">("list");

  // ── Template list state ─────────────────────────────────────────────────
  const [templates, setTemplates] = useState<SurveyTemplate[]>([]);
  const [listLoading, setListLoading] = useState(true);
  const [listError, setListError] = useState<string | null>(null);

  // ── Builder state ───────────────────────────────────────────────────────
  const [templateName, setTemplateName] = useState("");
  const [fields, setFields] = useState<SurveyField[]>([]);
  const [saving, setSaving] = useState(false);
  const [saveError, setSaveError] = useState<string | null>(null);
  const [saveSuccess, setSaveSuccess] = useState(false);
  const timeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  // ── Load templates ──────────────────────────────────────────────────────
  const loadTemplates = useCallback(async () => {
    setListLoading(true);
    setListError(null);
    try {
      const result = await listSurveyTemplates();
      setTemplates(result);
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      setListError(msg);
    } finally {
      setListLoading(false);
    }
  }, []);

  useEffect(() => {
    return () => {
      if (timeoutRef.current) clearTimeout(timeoutRef.current);
    };
  }, []);

  useEffect(() => {
    loadTemplates();
  }, [loadTemplates]);

  // ── Field manipulation handlers ─────────────────────────────────────────
  const handleAddField = useCallback(() => {
    const newField: SurveyField = {
      id: generateFieldId(),
      label: "",
      fieldType: "text",
      required: false,
      order: fields.length,
    };
    setFields((prev) => [...prev, newField]);
  }, [fields.length]);

  const handleFieldChange = useCallback(
    (id: string, updates: Partial<SurveyField>) => {
      setFields((prev) =>
        prev.map((f) => (f.id === id ? { ...f, ...updates } : f)),
      );
    },
    [],
  );

  const handleMoveUp = useCallback((id: string) => {
    setFields((prev) => {
      const idx = prev.findIndex((f) => f.id === id);
      if (idx <= 0) return prev;
      const next = [...prev];
      const temp = next[idx - 1];
      next[idx - 1] = next[idx];
      next[idx] = temp;
      return next.map((f, i) => ({ ...f, order: i }));
    });
  }, []);

  const handleMoveDown = useCallback((id: string) => {
    setFields((prev) => {
      const idx = prev.findIndex((f) => f.id === id);
      if (idx < 0 || idx >= prev.length - 1) return prev;
      const next = [...prev];
      const temp = next[idx + 1];
      next[idx + 1] = next[idx];
      next[idx] = temp;
      return next.map((f, i) => ({ ...f, order: i }));
    });
  }, []);

  const handleDeleteField = useCallback((id: string) => {
    setFields((prev) =>
      prev
        .filter((f) => f.id !== id)
        .map((f, i) => ({ ...f, order: i })),
    );
  }, []);

  // ── Save handler ────────────────────────────────────────────────────────
  const handleSave = useCallback(async () => {
    if (!templateName.trim()) {
      setSaveError("Template name is required.");
      return;
    }
    if (fields.length === 0) {
      setSaveError("Add at least one field.");
      return;
    }
    const emptyLabels = fields.filter((f) => !f.label.trim());
    if (emptyLabels.length > 0) {
      setSaveError("All fields must have a label.");
      return;
    }

    setSaving(true);
    setSaveError(null);
    setSaveSuccess(false);

    try {
      await createSurveyTemplate({
        name: templateName.trim(),
        fields: fields.map((f, i) => ({ ...f, order: i })),
      });
      setSaveSuccess(true);
      // Reset builder and go back to list
      setTemplateName("");
      setFields([]);
      await loadTemplates();
      // Stay on success for a moment, then switch to list
      if (timeoutRef.current) clearTimeout(timeoutRef.current);
      timeoutRef.current = setTimeout(() => {
        setSaveSuccess(false);
        setView("list");
      }, 1500);
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      setSaveError(msg);
    } finally {
      setSaving(false);
    }
  }, [templateName, fields, loadTemplates]);

  // ── Start new template ──────────────────────────────────────────────────
  const handleCreateNew = useCallback(() => {
    setTemplateName("");
    setFields([]);
    setSaveError(null);
    setSaveSuccess(false);
    setView("builder");
  }, []);

  // ── Render: Template List ─────────────────────────────────────────────────
  if (view === "list") {
    return (
      <div className="space-y-6 p-6">
        {/* Header */}
        <div className="flex items-start justify-between gap-4">
          <div className="flex items-center gap-3">
            <button
              type="button"
              onClick={goBack}
              className="rounded-md p-1.5 text-gray-500 hover:bg-gray-100 hover:text-gray-700 focus:outline-none focus:ring-2 focus:ring-gray-400 focus:ring-offset-1"
              aria-label="Go back"
            >
              ← Back
            </button>
            <div>
              <h1 className="text-xl font-bold text-gray-900">
                Survey Templates
              </h1>
              <p className="mt-0.5 text-sm text-gray-500">
                Manage intake survey templates for patient forms.
              </p>
            </div>
          </div>
          <button
            type="button"
            onClick={handleCreateNew}
            className="rounded-md bg-blue-600 px-4 py-2 text-sm font-medium text-white shadow-sm hover:bg-blue-700 focus:outline-none focus:ring-2 focus:ring-blue-500 focus:ring-offset-2"
          >
            Create New Template
          </button>
        </div>

        {/* Template list */}
        <SectionCard title="All Templates">
          {listLoading ? (
            <div className="animate-pulse space-y-3">
              <div className="h-10 rounded bg-gray-200" />
              <div className="h-10 rounded bg-gray-200" />
              <div className="h-10 rounded bg-gray-200" />
            </div>
          ) : listError ? (
            <div className="rounded-md border border-red-200 bg-red-50 px-4 py-3 text-sm text-red-700">
              <p className="font-semibold">Failed to load templates</p>
              <p className="mt-1">{listError}</p>
              <button
                type="button"
                onClick={loadTemplates}
                className="mt-2 rounded bg-red-600 px-3 py-1 text-xs font-medium text-white hover:bg-red-700"
              >
                Retry
              </button>
            </div>
          ) : templates.length === 0 ? (
            <p className="text-sm text-gray-500">
              No templates yet. Create one to get started.
            </p>
          ) : (
            <div className="overflow-x-auto">
              <table className="w-full text-sm">
                <thead>
                  <tr className="border-b border-gray-100 text-left text-xs font-medium uppercase tracking-wide text-gray-500">
                    <th className="pb-2 pr-4">Name</th>
                    <th className="pb-2 pr-4">Fields</th>
                    <th className="pb-2 pr-4">Type</th>
                    <th className="pb-2">Created</th>
                  </tr>
                </thead>
                <tbody className="divide-y divide-gray-50">
                  {templates.map((t) => (
                    <tr key={t.id} className="hover:bg-gray-50">
                      <td className="py-3 pr-4 font-medium text-gray-900">
                        {t.name}
                      </td>
                      <td className="py-3 pr-4 text-gray-600">
                        {t.fields.length} field{t.fields.length !== 1 ? "s" : ""}
                      </td>
                      <td className="py-3 pr-4">
                        {t.builtIn ? (
                          <span className="inline-flex rounded-full bg-blue-100 px-2 py-0.5 text-xs font-medium text-blue-800">
                            Built-in
                          </span>
                        ) : (
                          <span className="inline-flex rounded-full bg-gray-100 px-2 py-0.5 text-xs font-medium text-gray-600">
                            Custom
                          </span>
                        )}
                      </td>
                      <td className="py-3 text-gray-500">
                        {new Date(t.createdAt).toLocaleDateString()}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
        </SectionCard>
      </div>
    );
  }

  // ── Render: Builder View ──────────────────────────────────────────────────
  return (
    <div className="space-y-6 p-6">
      {/* Header */}
      <div className="flex items-start justify-between gap-4">
        <div className="flex items-center gap-3">
          <button
            type="button"
            onClick={() => setView("list")}
            className="rounded-md p-1.5 text-gray-500 hover:bg-gray-100 hover:text-gray-700 focus:outline-none focus:ring-2 focus:ring-gray-400 focus:ring-offset-1"
            aria-label="Back to templates list"
          >
            ← Back
          </button>
          <div>
            <h1 className="text-xl font-bold text-gray-900">
              Create Survey Template
            </h1>
            <p className="mt-0.5 text-sm text-gray-500">
              Define fields for a new intake survey.
            </p>
          </div>
        </div>
        <button
          type="button"
          onClick={handleSave}
          disabled={saving}
          className="rounded-md bg-blue-600 px-5 py-2 text-sm font-medium text-white shadow-sm hover:bg-blue-700 focus:outline-none focus:ring-2 focus:ring-blue-500 focus:ring-offset-2 disabled:opacity-60"
        >
          {saving ? "Saving..." : "Save Template"}
        </button>
      </div>

      {/* Save error / success */}
      {saveError && (
        <div className="rounded-md border border-red-200 bg-red-50 px-4 py-3 text-sm text-red-700">
          {saveError}
        </div>
      )}
      {saveSuccess && (
        <div className="rounded-md border border-green-200 bg-green-50 px-4 py-3 text-sm text-green-800">
          Template saved successfully!
        </div>
      )}

      {/* Template name */}
      <SectionCard title="Template Details">
        <label className="mb-1 block text-sm font-medium text-gray-700">
          Template Name
        </label>
        <input
          type="text"
          value={templateName}
          onChange={(e) => setTemplateName(e.target.value)}
          placeholder="e.g., New Patient Intake, Post-Surgery Follow-Up..."
          className="w-full max-w-lg rounded-md border border-gray-300 px-3 py-2 text-sm shadow-sm focus:border-blue-500 focus:outline-none focus:ring-1 focus:ring-blue-500"
        />
      </SectionCard>

      {/* Fields list */}
      <SectionCard title="Fields">
        {fields.length === 0 ? (
          <p className="mb-4 text-sm text-gray-500">
            No fields yet. Click "Add Field" to start building.
          </p>
        ) : (
          <div className="mb-4 space-y-3">
            {fields.map((field, index) => (
              <FieldRow
                key={field.id}
                field={field}
                index={index}
                total={fields.length}
                onChange={handleFieldChange}
                onMoveUp={handleMoveUp}
                onMoveDown={handleMoveDown}
                onDelete={handleDeleteField}
              />
            ))}
          </div>
        )}

        <button
          type="button"
          onClick={handleAddField}
          className="rounded-md border border-blue-300 bg-blue-50 px-4 py-2 text-sm font-medium text-blue-700 hover:bg-blue-100 focus:outline-none focus:ring-2 focus:ring-blue-500 focus:ring-offset-1"
        >
          + Add Field
        </button>
      </SectionCard>

      {/* Field type reference */}
      <SectionCard title="Field Type Reference">
        <div className="grid grid-cols-1 gap-2 text-sm sm:grid-cols-2 lg:grid-cols-3">
          <div className="rounded border border-gray-100 bg-gray-50 p-3">
            <p className="font-medium text-gray-800">Text</p>
            <p className="text-xs text-gray-500">Free-form text area for open-ended answers.</p>
          </div>
          <div className="rounded border border-gray-100 bg-gray-50 p-3">
            <p className="font-medium text-gray-800">Number</p>
            <p className="text-xs text-gray-500">Numeric input with +/- buttons.</p>
          </div>
          <div className="rounded border border-gray-100 bg-gray-50 p-3">
            <p className="font-medium text-gray-800">Yes / No</p>
            <p className="text-xs text-gray-500">Two large toggle buttons.</p>
          </div>
          <div className="rounded border border-gray-100 bg-gray-50 p-3">
            <p className="font-medium text-gray-800">Pain Scale (0-10)</p>
            <p className="text-xs text-gray-500">Horizontal 0-10 scale with color gradient.</p>
          </div>
          <div className="rounded border border-gray-100 bg-gray-50 p-3">
            <p className="font-medium text-gray-800">Date</p>
            <p className="text-xs text-gray-500">Date picker input.</p>
          </div>
        </div>
      </SectionCard>
    </div>
  );
}
