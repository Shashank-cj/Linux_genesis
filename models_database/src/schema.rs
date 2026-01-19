// @generated automatically by Diesel CLI.

diesel::table! {
    agent (uuid) {
        uuid -> Text,
        os -> Text,
        hostname -> Text,
        os_version -> Text,
    }
}

diesel::table! {
    agent_credential (id) {
        id -> Nullable<Integer>,
        uuid -> Text,
        client_id -> Text,
        client_secret -> Text,
        master_key -> Text,
    }
}

diesel::table! {
    cpu (uuid) {
        uuid -> Text,
        device_uuid -> Text,
        make -> Text,
        model -> Text,
        p_cores -> Integer,
        l_cores -> Integer,
        speed -> Text,
        os_uuid -> Nullable<Text>,
    }
}

diesel::table! {
    device (uuid) {
        uuid -> Text,
        make -> Text,
        model -> Text,
        serial_number -> Text,
        dev_phy_vm -> Text,
        reboot_time -> Nullable<Text>,
    }
}

diesel::table! {
    gpu (uuid) {
        uuid -> Text,
        device_uuid -> Text,
        make -> Text,
        model -> Text,
        serial_number -> Text,
        size -> Text,
        driver -> Text,
        os_uuid -> Nullable<Text>,
    }
}

diesel::table! {
    ip_address (uuid) {
        uuid -> Text,
        port_uuid -> Text,
        address -> Text,
        gateway -> Nullable<Text>,
        subnet_mask -> Text,
        dns -> Text,
        os_uuid -> Nullable<Text>,
    }
}

diesel::table! {
    memory (uuid) {
        uuid -> Text,
        device_uuid -> Text,
        make -> Text,
        model -> Text,
        speed -> Text,
        size -> Text,
        serial_number -> Text,
        os_uuid -> Nullable<Text>,
    }
}

diesel::table! {
    nic (uuid) {
        uuid -> Text,
        device_uuid -> Text,
        make -> Text,
        model -> Text,
        number_of_ports -> Integer,
        max_speed -> Text,
        supported_speeds -> Text,
        serial_number -> Text,
        mac_address -> Text,
        os_uuid -> Nullable<Text>,
    }
}

diesel::table! {
    partition (uuid) {
        uuid -> Text,
        storage_uuid -> Text,
        name -> Text,
        serial_number -> Text,
        fs_type -> Text,
        free_space -> Text,
        used_space -> Text,
        total_size -> Text,
        os_uuid -> Nullable<Text>,
    }
}

diesel::table! {
    port (uuid) {
        uuid -> Text,
        nic_uuid -> Text,
        interface_name -> Text,
        operating_speed -> Text,
        is_physical_logical -> Text,
        logical_type -> Text,
        os_uuid -> Nullable<Text>,
    }
}

diesel::table! {
    storage (uuid) {
        uuid -> Text,
        device_uuid -> Text,
        hw_disk_type -> Text,
        make -> Text,
        model -> Text,
        serial_number -> Text,
        base_fs_type -> Text,
        free_space -> Text,
        total_disk_usage -> Text,
        total_disk_size -> Text,
        unallocated_disk_size -> Text,
        os_uuid -> Nullable<Text>,
    }
}

diesel::table! {
    tokens (id) {
        id -> Nullable<Integer>,
        token -> Text,
        expiration -> Text,
        token_type -> Text,
    }
}

diesel::table! {
    secrets (key) {
        key -> Text,
        value -> Binary,
    }
}



diesel::joinable!(cpu -> device (device_uuid));
diesel::joinable!(gpu -> device (device_uuid));
diesel::joinable!(ip_address -> port (port_uuid));
diesel::joinable!(memory -> device (device_uuid));
diesel::joinable!(nic -> device (device_uuid));
diesel::joinable!(partition -> storage (storage_uuid));
diesel::joinable!(port -> nic (nic_uuid));
diesel::joinable!(storage -> device (device_uuid));

diesel::allow_tables_to_appear_in_same_query!(
    agent,
    agent_credential,
    cpu,
    device,
    gpu,
    ip_address,
    memory,
    nic,
    partition,
    port,
    storage,
    tokens,
    secrets,
);
