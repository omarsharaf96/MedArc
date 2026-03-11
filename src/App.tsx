function App() {
  return (
    <div className="flex min-h-screen items-center justify-center bg-gray-50">
      <div className="text-center">
        <h1 className="text-4xl font-bold text-gray-900">MedArc</h1>
        <p className="mt-2 text-lg text-gray-600">
          AI-Powered Desktop EMR
        </p>
        <div className="mt-6 flex items-center justify-center gap-2">
          <span className="inline-block h-3 w-3 rounded-full bg-gray-400" />
          <span className="text-sm text-gray-500">Connecting...</span>
        </div>
      </div>
    </div>
  );
}

export default App;
