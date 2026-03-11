import DatabaseStatus from "./components/DatabaseStatus";
import FhirExplorer from "./components/FhirExplorer";

function App() {
  return (
    <div className="min-h-screen bg-gray-50 p-8">
      <div className="mx-auto max-w-4xl">
        {/* Header */}
        <div className="mb-8 text-center">
          <h1 className="text-4xl font-bold text-gray-900">MedArc</h1>
          <p className="mt-1 text-lg text-gray-500">
            Electronic Medical Records
          </p>
        </div>

        {/* Database Status */}
        <div className="mb-6">
          <DatabaseStatus />
        </div>

        {/* FHIR Explorer */}
        <div className="mb-6">
          <FhirExplorer />
        </div>
      </div>
    </div>
  );
}

export default App;
