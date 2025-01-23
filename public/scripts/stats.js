let username;
let groups;

// Fetches the session data from `localhost:5777/api/v1/auth/me`,
//  then inject the username and user groups into the 'navbar'
async function build_navbar ( ) {
    let res = await fetch('api/v1/auth/me', {
        method: 'GET',
        credentials: 'include',   // <-- crucial to include cookies
    });
    let data = await res.json();

    if ( !data.username ) {
        console.log("No username found in session data.");

        return;
    }

    username = data.username;
    groups = data.groups;

    let navbar = document.getElementById('navbar');

    let user_item = document.createElement('div');
    user_item.classList.add('nav-item');
    user_item.innerHTML = `
    <a href="index.html?user=${username}">
        <h2>${username}</h2>
    </a>
    `;
    navbar.appendChild(user_item);

    let groups_item = document.createElement('div');
    groups_item.classList.add('nav-item');
    groups_item.classList.add('groups');
    groups_item.innerHTML = `<h2>Groups</h2>`;
    navbar.appendChild(groups_item);

    groups.forEach(group => {
        let group_item = document.createElement('div');
        group_item.classList.add('group-item');
        group_item.innerHTML = `
        <a href="index.html?group=${group}">${group}</a>
        `;
        groups_item.appendChild(group_item);
    });
}

// Checks if the username is populated, and if not,
//  renders the login button in the div ID 'auth' 
async function build_auth ( ) {
    if ( !username ) {
        let auth = document.getElementById('auth');
        auth.innerHTML = `
        <a href="https://www.niu.edu/crcd/prospective-user/access.shtml">
            <button class="signin-button"><b>Register</b></button>
        </a>
        <a href="login.html">
            <button class="signin-button"><b>Sign In</b></button>
        </a>
        `;
    } else {
        // Create a sign out button that, when clicked, sends
        //  a GET request to `localhost:5777/api/v1/auth/logout`
        let auth = document.getElementById('auth');
        auth.innerHTML = `
        <button class="signin-button" id="signout-button"><b>Sign Out</b></button>
        `;

        let signout_button = document.getElementById('signout-button');
        signout_button.addEventListener('click', async (event) => {
            console.log(":3");

            let res = await fetch('api/v1/auth/logout', {
                method: 'POST',
                credentials: 'include',
            });

            if ( res.ok ) {
                console.log("^^");
                location.reload();
            }
        }); 
    }
}


function getColor(value, max, min) {
    const percent = (value - min) / (max - min);
    const red = Math.min(255, Math.floor(255 * percent));
    const green = Math.min(205, Math.floor(205 * (1 - percent)));
    return `rgb(${red}, ${green}, 0)`;
}
function getIcon(label) {
    switch (label) {
        case 'Load (% per CPU)':
            return '🖥️'; // CPU icon
        case 'Memory Efficiency (%)':
            return '💾'; // RAM icon
        case 'Walltime Usage (%)':
            return '🕒'; // Clock icon
        default:
            return '';
    }
}
function createProgressBar(label, value, min, max, invert = false) {
    const percent = Math.max(((value - min) / (max - min)) * 100, 5);
    const icon = getIcon(label); // Get the appropriate icon for the label
    return `
    <div class="progress-container">
        <div style="display:flex;justify-content:space-between;">
            <strong>${label}</strong>
            <strong style="text-align:right;margin-left:auto">${icon}</strong>
        </div>
        <div class="progress-bar">
            <div class="progress-fill" style="width: ${percent}%; background-color: ${invert ? getColor(value, max, min) : getColor(value, min, max)};">
                <b>${value}%</b>
        </div>
    </div>
    `;
}

// Add this function to render the charts using Chart.js
function renderCharts(stats) {
    const ctxCpu = document.getElementById('cpuChart').getContext('2d');
    const ctxMem = document.getElementById('memChart').getContext('2d');

    const cpuData = stats.map(stat => parseFloat(stat.cpu_percent));
    const memData = stats.map(stat => parseFloat(stat.mem));

    const cpuChart = new Chart(ctxCpu, {
        type: 'line',
        data: {
            labels: stats.map((_, index) => `${stats[index].datetime}`),
            datasets: [{
                label: 'CPU Usage (%)',
                data: cpuData,
                borderColor: 'rgba(255, 99, 132, 1)',
                backgroundColor: 'rgba(255, 99, 132, 0.2)',
                fill: true,
            }]
        },
        options: {
            responsive: true,
            scales: {
                y: {
                    beginAtZero: true,
                    max: 100,
                }
            }
        }
    });
    const memChart = new Chart(ctxMem, {
        type: 'line',
        data: {
            labels: stats.map((_, index) => `${stats[index].datetime}`),
            datasets: [{
                label: 'Memory Usage (GB)',
                data: memData,
                borderColor: 'rgba(54, 162, 235, 1)',
                backgroundColor: 'rgba(54, 162, 235, 0.2)',
                fill: true,
            }]
        },
        options: {
            responsive: true,
            scales: {
                y: {
                    beginAtZero: true,
                }
            }
        }
    });
}
// Gets the jobs from 'localhost:5777/api/v1/jobs',
//  and adds them to the active jobs container
async function build_job ( ) {
    // First, check if a user or group was specified
    let url = new URL(window.location.href);
    let user = url.searchParams.get('user');
    let id = url.searchParams.get('id');
    console.log(":3");

    if ( !username ) {
        let alert_footer = document.getElementById('alert-footer');
        alert_footer.innerHTML = `You must be logged in to view detailed job stats!`;
        return;
    }

    let data;
    let data_res = await fetch(`api/v1/jobs?user=${user}`, {
        method: 'GET',
        credentials: 'include',
    });
    if ( data_res.status == 401 ) {
        let alert_footer = document.getElementById('alert-footer');
        alert_footer.innerHTML = `You are not authorized to view jobs for user '${user}'!`;
        return;
    } else if ( data_res.status == 200 ) {
        data = await data_res.json();
    } else {
        let err = await data_res.text();
        let alert_footer = document.getElementById('alert-footer');
        alert_footer.innerHTML = `An error occurred while fetching jobs for user '${user}'!<br>Error: ${err}`;
        return;
    }

    // Get the job with the specified ID
    let job = data.find(job => job.pbs_id == id);
    if ( !job ) {
        let alert_footer = document.getElementById('alert-footer');
        alert_footer.innerHTML = `No job found with ID '${id}'!`;
        return;
    }

    // Get the job's historical stats
    let stats;
    let stats_res = await fetch(`api/v1/stats?id=${id}`, {
        method: 'GET',
        credentials: 'include',
    });
    if ( stats_res.status == 401 ) {
        let alert_footer = document.getElementById('alert-footer');
        alert_footer.innerHTML = `You are not authorized to view jobs for user '${user}'!`;
        return;
    } else if ( stats_res.status == 200 ) {
        console.log("^^");
        stats = await stats_res.json();
    } else {
        let err = await stats_res.text();
        let alert_footer = document.getElementById('alert-footer');
        alert_footer.innerHTML = `An error occurred while fetching jobs for user '${user}'!<br>Error: ${err}`;
        return;
    }
    let end_time_header = "";
    let end_time_entry = "";
    if ( job.end_time ) {
        end_time_header = "<th>End Time</th>";
        end_time_entry = `<td>${job.end_time}</td>`;
    }
    let used_mem_per_core = parseFloat(job.used_mem) / parseFloat(job.req_cpus);

    // If there are 1 or fewer stats in either the CPU or memory,
    //  generate an error message
    let error_message = "";
    if ( stats.length <= 1 ) {
        error_message = `
        <div class="error-message">
            <p>
                <b>Insufficient Data</b>
                <br>
                There is insufficient data to generate the CPU and memory usage charts.
                This could be because the job was just submitted, and the stats have not been collected yet.
            </p>
        </div>
        `;
    }

    console.dir(job);
    console.dir(stats);

    // Set the inner HTML of the ID `job-card`
    let job_card = document.getElementById('job-card');
    job_card.innerHTML = `
    <div class="job-header">
        <p>
            <b>${job.name} - ${job.pbs_id} (${job.state})</b>
            <br>
            Submitted by <a href="index.html?user=${job.owner}">${job.owner}</a> on <b>${job.stime}</b>
        </p>
    </div>

    <table class="job-table">
        <thead>
            <tr>
                <th>Queue</th>
                <th>Walltime</th>
                <th># of CPUs</th>
                <th># of GPUs</th>
                <th>Memory</th>
                ${end_time_header}
            </tr>
        </thead>
        <tbody>
            <tr>
                <td>${job.queue}</td>
                <td>${job.req_walltime}</td>
                <td>${job.req_cpus}</td>
                <td>${job.req_gpus | 0}</td>
                <td>${job.req_mem}GB</td>
                ${end_time_entry}
            </tr>
        </tbody>
        <thead>
            <tr>
                <th></th>
                <th>Used CPU</th>
                <th>Used Mem</th>
                <th>Used Mem/CPU</th>
                <th>Used Walltime</th>
                <th></th>
            </tr>
        </thead>
        <tbody>
            <tr>
                <td></td>
                <td>${job.used_cpu_percent}%</td>
                <td>${job.used_mem}GB</td>
                <td>${used_mem_per_core.toFixed(2)}GB</td>
                <td>${job.used_walltime}</td>
                <td></td>
            </tr>
        </tbody>
    </table>
    <div class="job-nodes">
        <p>
            <b>Nodes</b>
            <br>
            ${job.nodes}
        </p>
        <p>
            <b>PBS Selection</b>
            <br>
            ${job.req_select}
        </p>
    </div>
    <div>
        ${createProgressBar('Load (% per CPU)', 
        Math.min(Math.ceil(parseFloat(job.cpu_efficiency)) + 1, 100), 
        0, 100)}
        ${createProgressBar('Memory Efficiency (%)', 
        Math.min(Math.ceil(parseFloat(job.mem_efficiency)) + 1, 100), 
        0, 100)}
        ${createProgressBar('Walltime Usage (%)', 
        Math.min(Math.floor(parseFloat(job.walltime_efficiency + 1)), 100), 
        0, 100, true)}
    </div>
    <canvas id="cpuChart" width="400" height="200"></canvas>
    <canvas id="memChart" width="400" height="200"></canvas>
    ${error_message}
    `;
    job_card.style.visibility = 'visible';

    if ( stats.length > 1 ) {
        renderCharts(stats);
    }
}

// Checks what the query type is, and adjusts the `job-section-header`
//  accordingly
async function build_section_header ( ) {
    let url = new URL(window.location.href);
    let user = url.searchParams.get('user');
    let id = url.searchParams.get('id');

    let header = document.getElementById('job-section-header');
    if ( user && id ) {
        header.innerHTML = `Extended Job Stats for Job ID '${id}' - Owned by <a href="index.html?user=${user}">'${user}'</a>`;
    } else {
        header.innerHTML = `Invalid Query Parameters`;
    }
    header.style.visibility = 'visible';
}

build_navbar()
    .then(() => build_section_header())
    .then(() => build_auth())
    .then(() => build_job());