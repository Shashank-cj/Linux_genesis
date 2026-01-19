use pyo3::prelude::*;
use pyo3::types::PyList;
use std::sync::Mutex;
use shared_config::CONFIG;

static AGENT_INSTANCE: Mutex<Option<Py<PyAny>>> = Mutex::new(None);
static MONITOR_INSTANCE: Mutex<Option<Py<PyAny>>> = Mutex::new(None);

fn get_class_instance<'a>(py: Python<'a>,instance_mutex: &Mutex<Option<Py<PyAny>>>,module_name: &str,class_name: &str,) -> PyResult<Py<PyAny>> {
    let mut lock = instance_mutex.lock().unwrap();

    if let Some(instance) = &*lock {
        return Ok(instance.clone());
    }

    let sys = py.import("sys")?;
    let path: &PyList = sys.getattr("path")?.downcast()?;

    path.insert(0, format!("{}/agent_collector", CONFIG.app_dir))?;
    path.insert(0,format!("{}/lib/python3.10/site-packages", CONFIG.python_venv_dir),)?;
    sys.setattr("executable",format!("{}/bin/python", CONFIG.python_venv_dir),)?;

    let module = py.import(module_name)?;
    let class = module.getattr(class_name)?;
    let instance = class.call0()?.into_py(py);

    *lock = Some(instance.clone());
    Ok(instance)
}


fn call_cached_method(instance: &Py<PyAny>, method_name: &str) -> PyResult<String> {
    Python::with_gil(|py| {
        let instance_ref = instance.as_ref(py);
        let result = instance_ref.call_method0(method_name)?;

        let json_module = py.import("json")?;
        let json_str: String = json_module.call_method1("dumps", (result,))?.extract()?;

        Ok(json_str)
    })
}

fn call_cached_method_with_args(instance: &Py<PyAny>, method_name: &str, arg: &str) -> PyResult<String> {
    Python::with_gil(|py| {
        let instance_ref = instance.as_ref(py);
        let result = instance_ref.call_method1(method_name, (arg,))?;

        let json_module = py.import("json")?;
        let json_str: String = json_module.call_method1("dumps", (result,))?.extract()?;

        Ok(json_str)
    })
}

fn clear_instance(instance_mutex: &Mutex<Option<Py<PyAny>>>) {
    let mut lock = instance_mutex.lock().unwrap();
    *lock = None;
}

pub fn agent_data() -> PyResult<String> {
    Python::with_gil(|py| {
        let instance = get_class_instance(py, &AGENT_INSTANCE, "agent_data", "AgentData")?;
        let result = call_cached_method(&instance, "get_full_info");
        clear_instance(&AGENT_INSTANCE);

        result
    })
}

pub fn agent_rescan() -> PyResult<String> {
    Python::with_gil(|py| {
        let instance = get_class_instance(py, &AGENT_INSTANCE, "agent_data", "AgentData")?;
        let result = call_cached_method(&instance, "reScan");
        clear_instance(&AGENT_INSTANCE);

        result
    })
}

pub fn monitor_data() -> PyResult<String> {
    Python::with_gil(|py| {
        let instance = get_class_instance(py, &MONITOR_INSTANCE, "monitoring_data", "MonitoringLinux")?;
        let result = call_cached_method(&instance, "get_monitoring_checkpoint");
        result
    })
}

pub fn scan_disk(action: &str) -> PyResult<String> {
    Python::with_gil(|py| {
        let instance = get_class_instance(py, &AGENT_INSTANCE, "agent_data", "AgentData")?;
        let result = call_cached_method_with_args(&instance, "scan_particular_action", action);
        clear_instance(&AGENT_INSTANCE);
        result
    })
}

pub fn scan_nic(action: &str) -> PyResult<String> {
    Python::with_gil(|py| {
        let instance = get_class_instance(py, &AGENT_INSTANCE, "agent_data", "AgentData")?;
        let result = call_cached_method_with_args(&instance, "scan_particular_action", action);
        clear_instance(&AGENT_INSTANCE);

        result
    })
}
