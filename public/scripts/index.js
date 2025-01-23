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
            let res = await fetch('api/v1/auth/logout', {
                method: 'POST',
                credentials: 'include',
            });

            if ( res.ok ) {
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

function createJobRow (job, index) {
    let node_text = job["nodes"]
        .split(',')
        .slice(0, 10)
        .join(', ');
    if ( job["nodes"].split(',').length > 10 ) {
        node_text += '... (' + (job["nodes"].split(',').length - 10) + ' more)';
    }

    const row = document.createElement('div');
    row.className = 'job-row';
    row.innerHTML = `
    <div class="job-card">
        <div class="job-header">
            <p>
                <b>${job.name} - ${job.pbs_id} (${job.state})</b>
                <br>
                Submitted by <a href="index.html?user=${job.owner}">${job.owner}</a> on <b>${new Date(parseInt(job.start_time)*1000).toLocaleString()}</b>
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
                </tr>
            </thead>
            <tbody>
                <tr>
                    <td>${job.queue}</td>
                    <td>${job.req_walltime}</td>
                    <td>${job.req_cpus}</td>
                    <td>${job.req_gpus | 0}</td>
                    <td>${job.req_mem}GB</td>
                </tr>
            </tbody>
        </table>

        <div class="job-nodes">
            <p>
                <b>Nodes</b>
                <br>
                ${node_text}
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
        <br>
        <a href="stats.html?user=${job.owner}&id=${job.pbs_id}">
            <button><b>View Detailed Stats</b></button>
        </a>
    </div>
    `;
    document.getElementById('running-jobs').appendChild(row);
}
// Gets the jobs from 'localhost:5777/api/v1/jobs',
//  and adds them to the active jobs container
async function build_jobs ( ) {
    // First, check if a user or group was specified
    let url = new URL(window.location.href);
    let user = url.searchParams.get('user');
    let group = url.searchParams.get('group');

    // Filters
    let owner = url.searchParams.get('owner');
    let state = url.searchParams.get('state');
    let queue = url.searchParams.get('queue');
    let name = url.searchParams.get('name');
    let date = url.searchParams.get('date');

    let additional_filters = '';
    if ( owner ) {
        additional_filters += `&owner=${owner}`;
    }
    if ( state ) {
        additional_filters += `&state=${state}`;
    }
    if ( queue ) {
        additional_filters += `&queue=${queue}`;
    }
    if ( name ) {
        additional_filters += `&name=${name}`;
    }
    if ( date ) {
        additional_filters += `&date=${date}`;
    }
    console.log(`Additional filters: ${additional_filters}`);

    let data;
    if ( user ) {
        // Inherit and pass parameters to the fetch call
        let res = await fetch(`api/v1/jobs?user=${user}${additional_filters}`, {
            method: 'GET',
            credentials: 'include',
        });

        if ( res.status == 401 ) {
            let alert_footer = document.getElementById('alert-footer');
            alert_footer.innerHTML = `You are not authorized to view jobs for user '${user}'!`;
            return;
        } else if ( res.status == 200 ) {
            data = await res.json();
        } else {
            let err = await res.text();
            let alert_footer = document.getElementById('alert-footer');
            alert_footer.innerHTML = `An error occurred while fetching jobs for user '${user}'!<br>Error: ${err}`;
            return;
        }
    } else if ( group ) {
        // Inherit and pass parameters to the fetch call
        let res = await fetch(`api/v1/jobs?group=${group}${additional_filters}`, {
            method: 'GET',
            credentials: 'include',
        });

        if ( res.status == 401 ) {
            let alert_footer = document.getElementById('alert-footer');
            alert_footer.innerHTML = `You are not authorized to view jobs for group '${group}'!`;
            return;
        } else if ( res.status == 200 ) {
            data = await res.json();
        } else {
            let err = await res.text();
            let alert_footer = document.getElementById('alert-footer');
            alert_footer.innerHTML = `An error occurred while fetching jobs for group '${group}'!<br>Error: ${err}`;
            return;
        }
    } else {
        // Inherit and pass parameters to the fetch call
        // (we need to remove the leading '&' and replace it with '?')
        additional_filters = additional_filters.replace('&', '?');
        let res = await fetch(`api/v1/jobs${additional_filters}`, {
            method: 'GET',
            credentials: 'include',
        });

        if ( res.status == 200 ) {
            data = await res.json();
        } else {
            let err = await res.text();
            let alert_footer = document.getElementById('alert-footer');
            alert_footer.innerHTML = `An error occurred while fetching jobs!<br>Error: ${err}`;
            return;
        }
    }
    console.dir(data);

    // If there are no jobs, populate the `alert-footer`
    //  with a message
    if ( data.length === 0 ) {
        let alert_footer = document.getElementById('alert-footer');
        alert_footer.innerHTML = `There were no jobs for the query and filters!`;
        return;
    }

    // Sort the data by the job ID
    data.sort((a, b) => parseInt(b.pbs_id) - parseInt(a.pbs_id));

    // Render the data
    data.forEach((job, index) => createJobRow(job, index));
}

// Checks what the query type is, and adjusts the `job-section-header`
//  accordingly
async function build_section_header ( ) {
    let url = new URL(window.location.href);
    let user = url.searchParams.get('user');
    let group = url.searchParams.get('group');

    // Get all filters
    let owner = url.searchParams.get('owner');
    let state = url.searchParams.get('state');
    let queue = url.searchParams.get('queue');
    let name = url.searchParams.get('name');
    let date = url.searchParams.get('date');
    let filters = [];

    let header = document.getElementById('job-section-header');
    if ( user ) {
        header.innerHTML = `<b>Metis User '${user}' Jobs</b>`;
    } else if ( group ) {
        header.innerHTML = `<b>Metis Group '${group}' Jobs</b>`;
    } else {
        header.innerHTML = `<b>All Jobs on Metis</b>`;
    }

    // Add filters to the header
    if ( owner ) { filters.push(`<b>Owner</b>: <i>${owner}</i>`); }
    if ( state ) { filters.push(`<b>State</b>: <i>${state}</i>`); } else { filters.push(`<b>State</b>: <i>R</i>`); }
    if ( queue ) { filters.push(`<b>Queue</b>: <i>${queue}</i>`); }
    if ( name ) { filters.push(`<b>Name</b>: <i>${name}</i>`); }
    if ( date ) { 
        // Build the timestamp for the date specified
        const now = new Date();
        const dateObj = new Date(parseInt(date) * 1000);
        filters.push(`<b>Date</b>: <i>${dateObj.toLocaleString()} - Now</i>`);
    } else { 
        // Build the timestamp for the past month
        const now = new Date();
        const past_month = new Date(now.getFullYear(), now.getMonth() - 1, now.getDate());
        filters.push(`<b>Date</b>: <i>${past_month.toLocaleString()} - Now</i>`);
    }

    if ( filters.length > 0 ) {
        header.innerHTML += ` - Filters - ${filters.join(', ')}`;
    }

    header.style.visibility = 'visible';
}

async function build_filter_section ( ) {
    console.log('Document is ready!');

    // Add the following element to the navbar:
    const filter_container = document.createElement('div');
    filter_container.className = 'filter-container';
    filter_container.id = 'filter-container';
    filter_container.style.visibility = 'hidden';
    filter_container.innerHTML = `
    <h3>Filters</h3>
    <input type="text" id="owner" placeholder="Owner" />
    <input type="text" id="queue" placeholder="Queue" />
    <input type="text" id="state" placeholder="State" />
    <input type="text" id="name" placeholder="Job Name" />

    <div>
    <label for="date-dropdown"><b>Date:</b></label>
    <select id="date-dropdown">
        <option value="all">All</option>
        <option value="day">Day</option>
        <option value="month">Month</option>
        <option value="year">Year</option>
    </select>
    </div>

    <br>

    <button id="search-button">Search</button>
    `;

    document.getElementById('navbar').appendChild(filter_container);

    // Set all of the example HTML items to their respective values
    //  if they are present in the query parameters
    const url = new URL(window.location.href);
    const owner = url.searchParams.get('owner');
    const queue = url.searchParams.get('queue');
    const state = url.searchParams.get('state');
    const name = url.searchParams.get('name');

    if (owner) document.getElementById('owner').value = owner;
    if (queue) document.getElementById('queue').value = queue;
    if (state) document.getElementById('state').value = state;
    if (name) document.getElementById('name').value = name;

    // Show the filter container (ID `filter-container`)
    if ( username ) {
        console.log("Showing filter container");
        document.getElementById('filter-container').style.visibility = 'visible';
    }

    document.getElementById('search-button').addEventListener('click', () => {
        console.log('Search button clicked');

        const owner = document.getElementById('owner').value;
        const queue = document.getElementById('queue').value;
        const state = document.getElementById('state').value;
        const name = document.getElementById('name').value;
        const dateOption = document.getElementById('date-dropdown').value;

        // Construct the new URL with filters
        let url = new URL(window.location.href);
        if (owner) url.searchParams.set('owner', owner);
        if (queue) url.searchParams.set('queue', queue);
        if (state) url.searchParams.set('state', state);
        if (name) url.searchParams.set('name', name);

        // Handle date selection
        const now = new Date();
        let dateParam;
        switch (dateOption) {
            case 'day':
                dateParam = Math.floor(now.setHours(0, 0, 0, 0) / 1000);
                break;
            case 'month':
                dateParam = Math.floor(new Date(now.getFullYear(), now.getMonth(), 1).getTime() / 1000);
                break;
            case 'year':
                dateParam = Math.floor(new Date(now.getFullYear(), 0, 1).getTime() / 1000);
                break;
            case 'all':
                // Ten years ago
                dateParam = Math.floor(new Date(now.getFullYear() - 10, now.getMonth(), now.getDate()).getTime() / 1000);
                break;
        }
        if (dateParam !== null) url.searchParams.set('date', dateParam);

        // Reload the page with the new query parameters
        window.location.href = url.toString();
    });
}

build_navbar()
    .then(() => build_section_header())
    .then(() => build_auth())
    .then(() => build_jobs())
    .then(() => build_filter_section());